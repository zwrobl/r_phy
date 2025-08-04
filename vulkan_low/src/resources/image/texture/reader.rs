use std::{borrow::Borrow, fs::File, io::Read, path::Path};

use ash::vk;
use graphics::model::Image;
use png::{BitDepth, ColorType, Transformations};
use strum::IntoEnumIterator;

use crate::{
    memory::MemoryProperties,
    resources::{
        error::ImageError,
        image::{Image2D, ImageCreateInfo, ImageCube, ImageInfo, ImageType},
    },
};

pub struct PngImageReader<R: Read> {
    reader: png::Reader<R>,
    exhausted: bool,
}

pub trait ImageReader {
    type Type: ImageType;

    fn required_buffer_size(&self) -> usize;
    fn get_create_info<M: MemoryProperties>(
        &self,
    ) -> Result<ImageCreateInfo<Self::Type, M>, ImageError>;
    fn read(&mut self, dst: &mut [u8]) -> Option<Result<u32, ImageError>>;
}

impl PngImageReader<File> {
    fn from_file(path: &Path) -> Result<Self, ImageError> {
        let mut decoder = png::Decoder::new(File::open(path)?);
        decoder.set_transformations(
            Transformations::EXPAND | Transformations::ALPHA | Transformations::STRIP_16,
        );
        Ok(Self {
            reader: decoder.read_info()?,
            exhausted: false,
        })
    }
}

impl<'a> PngImageReader<&'a [u8]> {
    fn from_buffer(image_data: &'a [u8]) -> Result<Self, ImageError> {
        let mut decoder = png::Decoder::new(image_data);
        decoder.set_transformations(
            Transformations::EXPAND | Transformations::ALPHA | Transformations::STRIP_16,
        );
        Ok(Self {
            reader: decoder.read_info()?,
            exhausted: false,
        })
    }
}

impl<R: Read> ImageReader for PngImageReader<R> {
    type Type = Image2D;

    fn read(&mut self, dst: &mut [u8]) -> Option<Result<u32, ImageError>> {
        if !self.exhausted {
            self.exhausted = true;
            let result = match self.reader.next_frame(dst) {
                Ok(_) => Ok(0),
                Err(err) => Err(ImageError::PngDecoderError(err)),
            };
            Some(result)
        } else {
            None
        }
    }

    fn get_create_info<M: MemoryProperties>(
        &self,
    ) -> Result<ImageCreateInfo<Image2D, M>, ImageError> {
        let info = self.reader.info();
        let extent = vk::Extent2D {
            width: info.width,
            height: info.height,
        };
        let format = match self.reader.output_color_type() {
            (ColorType::Rgba, BitDepth::Eight) => vk::Format::R8G8B8A8_SRGB,
            (ColorType::GrayscaleAlpha, BitDepth::Eight) => vk::Format::R8G8_SRGB,
            (color_type, bit_depth) => Err(ImageError::UnsupportedFormat(color_type, bit_depth))?,
        };
        Ok(ImageCreateInfo::new(ImageInfo {
            extent: extent.into(),
            format,
            usage: vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            samples: vk::SampleCountFlags::TYPE_1,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        })
        .with_mip_enabled())
    }

    fn required_buffer_size(&self) -> usize {
        self.reader.output_buffer_size()
    }
}

#[derive(strum::EnumIter, Debug, Clone, Copy, PartialEq)]
pub enum ImageCubeFace {
    Right = 0,
    Left = 1,
    Top = 2,
    Bottom = 3,
    Front = 4,
    Back = 5,
}

impl ImageCubeFace {
    fn get(path: &Path) -> Result<Self, ImageError> {
        let stem = path.file_stem().unwrap().to_string_lossy();
        let face = match stem.borrow() {
            "right" => Self::Right,
            "left" => Self::Left,
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            "front" => Self::Front,
            "back" => Self::Back,
            _ => Err(ImageError::InvalidCubeMap(stem.into_owned()))?,
        };
        Ok(face)
    }
}

pub struct ImageCubeReader {
    faces: Vec<(ImageCubeFace, PngImageReader<File>)>,
    next: usize,
}

impl ImageCubeReader {
    pub fn new(path: &Path) -> Result<Self, ImageError> {
        let faces = path
            .read_dir()?
            .filter_map(|entry| entry.map(|entry| entry.path()).ok())
            .filter(|path| path.is_file())
            .map(|path| {
                Ok((
                    ImageCubeFace::get(&path)?,
                    PngImageReader::from_file(&path)?,
                ))
            })
            .collect::<Result<Vec<_>, ImageError>>()?;
        if let Some(req) =
            ImageCubeFace::iter().find(|req| !faces.iter().any(|(face, _)| req == face))
        {
            Err(ImageError::MissingCubeMapData(req))?;
        }
        Ok(Self { faces, next: 0 })
    }
}

impl ImageReader for ImageCubeReader {
    type Type = ImageCube;

    fn required_buffer_size(&self) -> usize {
        let (_, reader) = &self.faces.first().unwrap();
        reader.required_buffer_size()
    }

    fn get_create_info<M: MemoryProperties>(
        &self,
    ) -> Result<ImageCreateInfo<Self::Type, M>, ImageError> {
        let (_, reader) = &self.faces.first().ok_or(ImageError::ExhaustedImageRead)?;
        let ImageCreateInfo { image_info, .. } = reader.get_create_info::<M>()?;
        Ok(ImageCreateInfo::new(image_info)
            .with_mip_enabled()
            .with_array_layers(0, 6))
    }

    fn read(&mut self, dst: &mut [u8]) -> Option<Result<u32, ImageError>> {
        let index = self.next;
        self.next += 1;
        self.faces.get_mut(index).and_then(|(face_index, reader)| {
            reader
                .read(dst)
                .map(|result| result.map(|_| *face_index as u32))
        })
    }
}

pub enum Image2DReader<'a> {
    File(PngImageReader<File>),
    Buffer(PngImageReader<&'a [u8]>),
}

impl<'a> Image2DReader<'a> {
    #[inline]
    pub fn new(image: &'a Image) -> Result<Self, ImageError> {
        let reader = match image {
            Image::File(path) => Self::File(PngImageReader::from_file(path)?),
            Image::Buffer(data) => Self::Buffer(PngImageReader::from_buffer(data)?),
        };
        Ok(reader)
    }
}

impl<'a> ImageReader for Image2DReader<'a> {
    type Type = Image2D;

    fn required_buffer_size(&self) -> usize {
        match self {
            Image2DReader::File(reader) => reader.required_buffer_size(),
            Image2DReader::Buffer(reader) => reader.required_buffer_size(),
        }
    }

    fn get_create_info<M: MemoryProperties>(
        &self,
    ) -> Result<ImageCreateInfo<Self::Type, M>, ImageError> {
        match self {
            Image2DReader::File(reader) => reader.get_create_info(),
            Image2DReader::Buffer(reader) => reader.get_create_info(),
        }
    }

    fn read(&mut self, dst: &mut [u8]) -> Option<Result<u32, ImageError>> {
        match self {
            Image2DReader::File(reader) => reader.read(dst),
            Image2DReader::Buffer(reader) => reader.read(dst),
        }
    }
}
