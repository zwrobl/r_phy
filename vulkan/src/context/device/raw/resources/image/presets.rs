use ash::vk;
use type_kit::Create;

use crate::context::{
    device::{
        memory::DeviceLocal,
        raw::resources::image::{Image2D, ImageCreateInfo, ImageInfo, ImagePartial},
    },
    error::ResourceResult,
    Context,
};

impl Context {
    pub fn prepare_color_attachment_image(
        &self,
    ) -> ResourceResult<ImagePartial<Image2D, DeviceLocal>> {
        let extent = self.physical_device.surface_properties.get_current_extent();
        ImagePartial::create(
            ImageCreateInfo::new(ImageInfo {
                extent: vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                },
                format: self.physical_device.attachment_properties.formats.color,
                samples: self.physical_device.attachment_properties.msaa_samples,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT
                    | vk::ImageUsageFlags::INPUT_ATTACHMENT,
                aspect: vk::ImageAspectFlags::COLOR,
                ..Default::default()
            }),
            self,
        )
    }

    pub fn prepare_depth_stencil_attachment_image(
        &self,
    ) -> ResourceResult<ImagePartial<Image2D, DeviceLocal>> {
        let extent = self.physical_device.surface_properties.get_current_extent();
        ImagePartial::create(
            ImageCreateInfo::new(ImageInfo {
                extent: vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                },
                format: self
                    .physical_device
                    .attachment_properties
                    .formats
                    .depth_stencil,
                samples: self.physical_device.attachment_properties.msaa_samples,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                    | vk::ImageUsageFlags::INPUT_ATTACHMENT,
                aspect: vk::ImageAspectFlags::DEPTH,
                ..Default::default()
            }),
            self,
        )
    }
}
