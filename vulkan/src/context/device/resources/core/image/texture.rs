use std::convert::Infallible;

use ash::vk;
use type_kit::{Create, CreateResult, Destroy, DestroyResult};

use crate::context::{
    device::{
        memory::{AllocReq, DeviceLocal},
        raw::allocator::AllocatorIndex,
        resources::{
            buffer::{StagingBuffer, StagingBufferBuilder},
            PartialBuilder,
        },
    },
    error::{VkError, VkResult},
    Context,
};

use super::{Image2D, Image2DBuilder, Image2DPartial, ImageReader};

pub struct Texture2DPartial<'a> {
    image: Image2DPartial<DeviceLocal>,
    reader: ImageReader<'a>,
}

pub struct Texture2D {
    pub image: Image2D<DeviceLocal>,
    pub sampler: vk::Sampler,
}

impl From<&Texture2D> for vk::DescriptorImageInfo {
    fn from(texture: &Texture2D) -> Self {
        vk::DescriptorImageInfo {
            sampler: texture.sampler,
            image_view: texture.image.image_view,
            image_layout: texture.image.layout,
        }
    }
}

impl<'a> PartialBuilder<'a> for Texture2DPartial<'a> {
    type Config = ImageReader<'a>;
    type Target = Texture2D;

    fn prepare(config: Self::Config, context: &Context) -> VkResult<Self> {
        let image = Image2DPartial::prepare(Image2DBuilder::new(config.info()?), context)?;
        Ok(Texture2DPartial {
            image,
            reader: config,
        })
    }

    fn requirements(&self) -> impl Iterator<Item = AllocReq> {
        self.image.requirements()
    }
}

impl Create for Texture2D {
    type Config<'a> = (Texture2DPartial<'a>, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (Texture2DPartial { image, mut reader }, allocator) = config;
        let mut image = Image2D::create((image, allocator), context)?;
        let mut builder = StagingBufferBuilder::new();
        let image_range = builder.append::<u8>(reader.required_buffer_size()?);
        {
            let mut staging_buffer =
                StagingBuffer::create((builder, context.default_allocator()), context)?;
            let mut image_range = staging_buffer.write_range::<u8>(image_range);
            let staging_area = image_range.remaining_as_slice_mut();
            while let Some(dst_layer) = reader.read(staging_area)? {
                staging_buffer.transfer_image_data(
                    &context,
                    &mut image,
                    dst_layer,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                )?;
            }
            image.layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            let _ = staging_buffer.destroy(context);
        }
        let create_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .border_color(vk::BorderColor::FLOAT_OPAQUE_BLACK)
            .min_lod(0.0)
            .max_lod(image.mip_levels as f32);
        let sampler = unsafe { context.create_sampler(&create_info, None)? };
        Ok(Texture2D { image, sampler })
    }
}

impl Destroy for Texture2D {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_sampler(self.sampler, None);
        }
        let _ = self.image.destroy(context);
        Ok(())
    }
}
