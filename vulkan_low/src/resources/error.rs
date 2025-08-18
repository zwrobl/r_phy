use std::{
    error::Error,
    fmt::Debug,
    fmt::{Display, Formatter},
    io,
};

use ash::vk;
use graphics::error::GraphicsError;
use png::{BitDepth, ColorType};

use crate::{
    error::{ExtError, SafeGuardError},
    memory::error::MemoryError,
    resources::{
        buffer::BufferRaw,
        command::PersistentCommandPoolRaw,
        descriptor::DescriptorPoolDataRaw,
        framebuffer::FramebufferRaw,
        image::{ImageCubeFace, ImageRaw, TextureRaw},
        pipeline::GraphicsPipelineRaw,
        swapchain::SwapchainRaw,
    },
};

#[derive(Debug)]
pub enum GuardError {
    Buffer {
        error: Box<SafeGuardError<BufferRaw>>,
    },
    Image {
        error: Box<SafeGuardError<ImageRaw>>,
    },
    Texture {
        error: Box<SafeGuardError<TextureRaw>>,
    },
    Swapchain {
        error: Box<SafeGuardError<SwapchainRaw>>,
    },
    GraphicsPipeline {
        error: SafeGuardError<GraphicsPipelineRaw>,
    },
    DescriptorPool {
        error: SafeGuardError<DescriptorPoolDataRaw>,
    },
    PersistentCommandPool {
        error: SafeGuardError<PersistentCommandPoolRaw>,
    },
    Framebuffer {
        error: SafeGuardError<FramebufferRaw>,
    },
}

impl Display for GuardError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardError::Buffer { error } => {
                write!(f, "Buffer type conversion error: {:?}", error)
            }
            GuardError::Image { error } => {
                write!(f, "Image type conversion error: {:?}", error)
            }
            GuardError::Texture { error } => {
                write!(f, "Texture type conversion error: {:?}", error)
            }
            GuardError::GraphicsPipeline { error } => {
                write!(f, "GraphicsPipeline type conversion error: {:?}", error)
            }
            GuardError::DescriptorPool { error } => {
                write!(f, "DescriptorPool type conversion error: {:?}", error)
            }
            GuardError::PersistentCommandPool { error } => write!(
                f,
                "PersistentCommandPool type conversion error: {:?}",
                error
            ),
            GuardError::Framebuffer { error } => {
                write!(f, "Framebuffer type conversion error: {:?}", error)
            }
            GuardError::Swapchain { error } => {
                write!(f, "Swapchain type conversion error: {:?}", error)
            }
        }
    }
}

impl Error for GuardError {}

#[derive(Debug)]
pub enum ShaderError {
    UnknowStage(String),
    InvalidFile(String),
    FileError(io::Error),
    VkError(vk::Result),
}

impl Display for ShaderError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            ShaderError::UnknowStage(stage) => {
                write!(f, "Unknown shader file type extension: {}!", stage)
            }
            ShaderError::InvalidFile(file) => {
                write!(
                    f,
                    "Shader module path is mising file name component: {}",
                    file
                )
            }
            ShaderError::FileError(err) => write!(f, "File error: {}", err),
            ShaderError::VkError(err) => write!(f, "Vulkan error: {}", err),
        }
    }
}

impl Error for ShaderError {}

pub type ShaderResult<T> = Result<T, ShaderError>;

impl From<io::Error> for ShaderError {
    fn from(err: io::Error) -> Self {
        ShaderError::FileError(err)
    }
}

impl From<vk::Result> for ShaderError {
    fn from(err: vk::Result) -> Self {
        ShaderError::VkError(err)
    }
}

#[derive(Debug)]
pub enum ImageError {
    FileError(io::Error),
    PngDecoderError(png::DecodingError),
    UnsupportedFormat(ColorType, BitDepth),
    InvalidCubeMap(String),
    MissingCubeMapData(ImageCubeFace),
    ExhaustedImageRead,
}

impl Display for ImageError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            ImageError::ExhaustedImageRead => write!(f, "Exhausted image read"),
            ImageError::MissingCubeMapData(entry) => {
                write!(f, "Missing cubemap entry: {:?}", entry)
            }
            ImageError::InvalidCubeMap(entry) => {
                write!(f, "Invalid cubemap directory entry: {}", entry)
            }
            ImageError::FileError(err) => write!(f, "File error: {}", err),
            ImageError::PngDecoderError(err) => write!(f, "PNG decoder error: {}", err),
            ImageError::UnsupportedFormat(color_type, bit_depth) => {
                write!(
                    f,
                    "Unsupported png Image ColorType: {:?} and BitDepth: {:?}!",
                    color_type, bit_depth
                )
            }
        }
    }
}

impl Error for ImageError {}

impl From<io::Error> for ImageError {
    fn from(err: io::Error) -> Self {
        ImageError::FileError(err)
    }
}

impl From<png::DecodingError> for ImageError {
    fn from(err: png::DecodingError) -> Self {
        ImageError::PngDecoderError(err)
    }
}

pub type ImageResult<T> = Result<T, ImageError>;

#[derive(Debug)]
pub enum ResourceError {
    MemoryError(MemoryError),
    ImageError(ImageError),
    ShaderError(ShaderError),
    GuardError(GuardError),
    ExtError(ExtError),
}

impl Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceError::MemoryError(error) => write!(f, "{}", error),
            ResourceError::GuardError(error) => write!(f, "{}", error),
            ResourceError::ImageError(error) => write!(f, "{}", error),
            ResourceError::ExtError(error) => write!(f, "{}", error),
            ResourceError::ShaderError(error) => write!(f, "{}", error),
        }
    }
}

impl Error for ResourceError {}

impl From<ShaderError> for ResourceError {
    fn from(error: ShaderError) -> Self {
        ResourceError::ShaderError(error)
    }
}

impl From<MemoryError> for ResourceError {
    fn from(error: MemoryError) -> Self {
        ResourceError::MemoryError(error)
    }
}

impl From<ImageError> for ResourceError {
    fn from(error: ImageError) -> Self {
        ResourceError::ImageError(error)
    }
}

impl<E: Into<ExtError>> From<E> for ResourceError {
    fn from(error: E) -> Self {
        ResourceError::ExtError(error.into())
    }
}

pub type ResourceResult<T> = Result<T, ResourceError>;

impl From<ResourceError> for GraphicsError {
    fn from(error: ResourceError) -> Self {
        GraphicsError::External(error.to_string())
    }
}
