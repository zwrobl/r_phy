use ash::vk;

use crate::device::AttachmentProperties;

use super::{Attachment, AttachmentFormatInfo, ClearColor, ClearDeptStencil, ClearNone};

pub struct ColorMultisampled {}

impl Attachment for ColorMultisampled {
    type Clear = ClearColor;

    fn get_format(properties: &AttachmentProperties) -> AttachmentFormatInfo {
        AttachmentFormatInfo {
            format: properties.formats.color,
            samples: properties.msaa_samples,
        }
    }
}

pub struct DepthStencilMultisampled {}

impl Attachment for DepthStencilMultisampled {
    type Clear = ClearDeptStencil;

    fn get_format(properties: &AttachmentProperties) -> AttachmentFormatInfo {
        AttachmentFormatInfo {
            format: properties.formats.depth_stencil,
            samples: properties.msaa_samples,
        }
    }
}

pub struct Resolve {}

impl Attachment for Resolve {
    type Clear = ClearNone;

    fn get_format(properties: &AttachmentProperties) -> AttachmentFormatInfo {
        AttachmentFormatInfo {
            format: properties.formats.color,
            samples: vk::SampleCountFlags::TYPE_1,
        }
    }
}
