mod reader;
mod texture;

use crate::context::{
    device::{
        memory::{AllocReq, AllocReqTyped, BindResource, DeviceLocal, MemoryProperties},
        raw::allocator::{AllocationEntry, AllocatorIndex},
    },
    error::{VkError, VkResult},
    Context,
};

use super::PartialBuilder;
use ash::vk;
use std::convert::Infallible;
use type_kit::{Create, Destroy, DestroyResult};

pub use reader::*;
pub use texture::*;

#[derive(Debug, Clone, Copy)]
struct Image2DInfo {
    extent: vk::Extent2D,
    format: vk::Format,
    flags: vk::ImageCreateFlags,
    samples: vk::SampleCountFlags,
    usage: vk::ImageUsageFlags,
    aspect_mask: vk::ImageAspectFlags,
    view_type: vk::ImageViewType,
    array_layers: u32,
    mip_levels: u32,
}

pub struct Image2DBuilder {
    info: Image2DInfo,
}

impl<'a, M: MemoryProperties> PartialBuilder<'a> for Image2DPartial<M> {
    type Config = Image2DBuilder;
    type Target = Image2D<M>;

    fn prepare(config: Self::Config, context: &Context) -> VkResult<Self> {
        let info = config.info;
        let image_info = vk::ImageCreateInfo::builder()
            .flags(info.flags)
            .extent(vk::Extent3D {
                width: info.extent.width,
                height: info.extent.height,
                depth: 1,
            })
            .format(info.format)
            .image_type(vk::ImageType::TYPE_2D)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .mip_levels(info.mip_levels)
            .array_layers(info.array_layers)
            .samples(info.samples)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(info.usage);
        let image = unsafe { context.create_image(&image_info, None)? };
        let req = BindResource::new(image).get_alloc_req(context);
        Ok(Image2DPartial { image, info, req })
    }

    fn requirements(&self) -> impl Iterator<Item = AllocReq> {
        [self.req.into()].into_iter()
    }
}

impl Image2DBuilder {
    fn new(info: Image2DInfo) -> Self {
        Self { info }
    }
}

pub struct Image2DPartial<M: MemoryProperties> {
    image: vk::Image,
    info: Image2DInfo,
    req: AllocReqTyped<M>,
}

pub struct Image2D<M: MemoryProperties> {
    pub array_layers: u32,
    pub mip_levels: u32,
    pub layout: vk::ImageLayout,
    pub extent: vk::Extent2D,
    pub image: vk::Image,
    pub image_view: vk::ImageView,
    allocation: AllocationEntry<M>,
}

impl Context {
    pub fn prepare_color_attachment_image(&self) -> VkResult<Image2DPartial<DeviceLocal>> {
        let extent = self.physical_device.surface_properties.get_current_extent();
        Image2DPartial::prepare(
            Image2DBuilder::new(Image2DInfo {
                extent,
                format: self.physical_device.attachment_properties.formats.color,
                flags: vk::ImageCreateFlags::empty(),
                samples: self.physical_device.attachment_properties.msaa_samples,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT
                    | vk::ImageUsageFlags::INPUT_ATTACHMENT,
                aspect_mask: vk::ImageAspectFlags::COLOR,
                view_type: vk::ImageViewType::TYPE_2D,
                array_layers: 1,
                mip_levels: 1,
            }),
            self,
        )
    }

    pub fn prepare_depth_stencil_attachment_image(&self) -> VkResult<Image2DPartial<DeviceLocal>> {
        let extent = self.physical_device.surface_properties.get_current_extent();
        Image2DPartial::prepare(
            Image2DBuilder::new(Image2DInfo {
                extent,
                format: self
                    .physical_device
                    .attachment_properties
                    .formats
                    .depth_stencil,
                flags: vk::ImageCreateFlags::empty(),
                samples: self.physical_device.attachment_properties.msaa_samples,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                    | vk::ImageUsageFlags::INPUT_ATTACHMENT,
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                view_type: vk::ImageViewType::TYPE_2D,
                array_layers: 1,
                mip_levels: 1,
            }),
            self,
        )
    }
}

impl<M: MemoryProperties> Create for Image2D<M> {
    type Config<'a> = (Image2DPartial<M>, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (
            Image2DPartial {
                image, info, req, ..
            },
            allocator,
        ) = config;
        let allocation = context.allocate(allocator, req)?;
        context.bind_memory(image, allocation)?;
        let view_info = vk::ImageViewCreateInfo::builder()
            .components(vk::ComponentMapping::default())
            .format(info.format)
            .image(image)
            .view_type(info.view_type)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: info.aspect_mask,
                base_mip_level: 0,
                level_count: info.mip_levels,
                base_array_layer: 0,
                layer_count: info.array_layers,
            });
        let image_view = unsafe { context.create_image_view(&view_info, None)? };
        Ok(Image2D {
            array_layers: info.array_layers,
            mip_levels: info.mip_levels,
            layout: vk::ImageLayout::UNDEFINED,
            extent: info.extent,
            image,
            image_view,
            allocation,
        })
    }
}

impl<M: MemoryProperties> Destroy for Image2D<M> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_image_view(self.image_view, None);
            context.destroy_image(self.image, None);
            let _ = context.free(self.allocation);
        }
        Ok(())
    }
}
