use std::{convert::Infallible, fmt::Debug, marker::PhantomData};

use ash::vk;
use type_kit::{Create, CreateResult, Destroy, DestroyResult, FromGuard};

use crate::{
    memory::MemoryProperties,
    resources::{
        error::ResourceError,
        image::{ArrayInfo, ImageCreateInfo, ImageInfo, ImageType, MipInfo},
    },
    Context,
};

pub struct ImageViewCreateInfo<V: ImageType> {
    image: vk::Image,
    format: vk::Format,
    aspect: vk::ImageAspectFlags,
    mip_info: MipInfo,
    array_info: ArrayInfo,
    _phantom: PhantomData<V>,
}

impl<V: ImageType> Default for ImageViewCreateInfo<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: ImageType> ImageViewCreateInfo<V> {
    pub fn new() -> Self {
        Self {
            image: vk::Image::null(),
            format: vk::Format::UNDEFINED,
            aspect: vk::ImageAspectFlags::default(),
            mip_info: MipInfo::default(),
            array_info: ArrayInfo::default(),
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn with_image(self, image: vk::Image) -> Self {
        Self { image, ..self }
    }

    pub fn with_format(self, format: vk::Format) -> Self {
        Self { format, ..self }
    }
    pub fn with_aspect(self, aspect: vk::ImageAspectFlags) -> Self {
        Self { aspect, ..self }
    }
    pub fn with_mip_info(self, mip_info: MipInfo) -> Self {
        Self { mip_info, ..self }
    }
    pub fn with_array_info(self, array_info: ArrayInfo) -> Self {
        Self { array_info, ..self }
    }

    fn get_vk_create_info(&self) -> vk::ImageViewCreateInfo {
        let ArrayInfo {
            base_array_layer,
            layer_count,
        } = self.array_info;
        let MipInfo {
            level_count,
            base_mip_level,
        } = self.mip_info;
        vk::ImageViewCreateInfo {
            view_type: V::VIEW_TYPE,
            image: self.image,
            format: self.format,
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: self.aspect,
                base_mip_level,
                level_count,
                base_array_layer,
                layer_count,
            },
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub struct ImageView<V: ImageType> {
    handle: vk::ImageView,
    _phantom: PhantomData<V>,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageViewRaw {
    handle: vk::ImageView,
}

impl<V: ImageType> ImageView<V> {
    pub fn get_vk_image_view(&self) -> vk::ImageView {
        self.handle
    }
}

impl<V: ImageType> FromGuard for ImageView<V> {
    type Inner = ImageViewRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        ImageViewRaw {
            handle: self.handle,
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            handle: inner.handle,
            _phantom: PhantomData,
        }
    }
}

impl<V: ImageType, M: MemoryProperties> ImageCreateInfo<V, M> {
    pub(crate) fn get_view_create_info(&self, image: vk::Image) -> ImageViewCreateInfo<V> {
        let ImageInfo { aspect, format, .. } = self.image_info;
        let array_info = self.image_info.array_info.unwrap_or_default();
        let mip_info = self.image_info.mip_info.unwrap_or_default();
        ImageViewCreateInfo {
            image,
            aspect,
            format,
            mip_info,
            array_info,
            _phantom: PhantomData,
        }
    }
}

impl<V: ImageType> Create for ImageView<V> {
    type Config<'a> = ImageViewCreateInfo<V>;
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let handle = unsafe { context.create_image_view(&config.get_vk_create_info(), None)? };
        Ok(Self {
            handle,
            _phantom: PhantomData,
        })
    }
}

impl<V: ImageType> Destroy for ImageView<V> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_image_view(self.handle, None);
        }
        Ok(())
    }
}

impl Destroy for ImageViewRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_image_view(self.handle, None);
        }
        Ok(())
    }
}
