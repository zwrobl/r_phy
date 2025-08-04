mod reader;

pub use reader::*;

use std::convert::Infallible;

use ash::vk;
use type_kit::{Create, CreateResult, Destroy, DestroyResult, DropGuard, FromGuard, GuardVec};

use crate::{
    memory::{
        allocator::{AllocatorBuilder, AllocatorIndex},
        DeviceLocal,
    },
    resources::{
        buffer::{StagingBuffer, StagingBufferBuilder, StagingBufferPartial},
        error::{GuardError, ResourceError},
        image::{
            sampler, Image, ImagePartial, ImageRaw, ImageType, Linear, Sampler, SamplerCreateInfo,
            SamplerRaw,
        },
        Partial, Resource, ResourceGuardError,
    },
    Context,
};

pub struct TexturePartial<V: ImageType, R: ImageReader<Type = V>> {
    image: DropGuard<ImagePartial<V, DeviceLocal>>,
    reader: R,
}

impl<V: ImageType, R: ImageReader<Type = V>> Create for TexturePartial<V, R> {
    type Config<'a> = R;

    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let image = DropGuard::new(ImagePartial::create(config.get_create_info()?, context)?);
        Ok(TexturePartial {
            image,
            reader: config,
        })
    }
}

impl<V: ImageType, R: ImageReader<Type = V>> Partial for TexturePartial<V, R> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.image.register_memory_requirements(builder);
    }
}

impl<V: ImageType, R: ImageReader<Type = V>> Destroy for TexturePartial<V, R> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.image.destroy(context);
        Ok(())
    }
}

#[derive(Debug)]
pub struct Texture<V: ImageType> {
    image: Image<V, DeviceLocal>,
    sampler: Sampler<Linear, sampler::Repeat>,
}

#[derive(Debug, Clone, Copy)]
pub struct TextureRaw {
    image: ImageRaw,
    sampler: SamplerRaw,
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorImageInfo {
    pub(crate) image_info: vk::DescriptorImageInfo,
}

impl DescriptorImageInfo {
    #[inline]
    pub fn get_vk_descriptor_image_info(&self) -> vk::DescriptorImageInfo {
        self.image_info
    }
}

impl<V: ImageType> From<&Texture<V>> for DescriptorImageInfo {
    #[inline]
    fn from(texture: &Texture<V>) -> Self {
        DescriptorImageInfo {
            image_info: vk::DescriptorImageInfo {
                sampler: texture.sampler.get_vk_sampler(),
                image_view: texture.image.view.get_vk_image_view(),
                image_layout: texture.image.layout,
            },
        }
    }
}

impl<V: ImageType> Create for Texture<V> {
    type Config<'a> = (
        DropGuard<TexturePartial<V, V::ImageReader<'a>>>,
        Option<AllocatorIndex>,
    );
    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (texture_partial, allocator) = config;
        let TexturePartial { image, mut reader } = unsafe { texture_partial.unwrap() };
        let mut image = Image::create((image, allocator), context)?;
        let mut builder = StagingBufferBuilder::new();
        let image_range = builder.append::<u8>(reader.required_buffer_size());
        let stating_buffer_partial =
            DropGuard::new(StagingBufferPartial::create(builder, context)?);
        {
            let mut staging_buffer =
                StagingBuffer::create((stating_buffer_partial, None), context)?;
            let mut image_range = staging_buffer.write_range::<u8>(image_range);
            let staging_area = image_range.remaining_as_slice_mut();
            while let Some(read_result) = reader.read(staging_area) {
                let dst_layer = read_result?;
                staging_buffer.transfer_image_data(
                    context,
                    &mut image,
                    dst_layer,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                )?;
            }
            image.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            let _ = staging_buffer.destroy(context);
        }
        let sampler_create_info =
            SamplerCreateInfo::<Linear, sampler::Repeat>::new(&image.image_info);
        let sampler = Sampler::create(sampler_create_info, context)?;
        Ok(Texture { image, sampler })
    }
}

impl<V: ImageType> Destroy for Texture<V> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.sampler.destroy(context);
        let _ = self.image.destroy(context);
        Ok(())
    }
}

impl Destroy for TextureRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.sampler.destroy(context);
        let _ = self.image.destroy(context);
        Ok(())
    }
}

impl<V: ImageType> FromGuard for Texture<V> {
    type Inner = TextureRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner {
            image: self.image.into_inner(),
            sampler: self.sampler.into_inner(),
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            image: Image::from_inner(inner.image),
            sampler: Sampler::from_inner(inner.sampler),
        }
    }
}

impl<V: ImageType> Resource for Texture<V> {
    type RawType = TextureRaw;
    type RawCollection = GuardVec<Self::RawType>;

    #[inline]
    fn wrap_guard_error(error: ResourceGuardError<Self>) -> ResourceError {
        ResourceError::GuardError(GuardError::Texture { error })
    }
}
