use std::{convert::Infallible, fmt::Debug, marker::PhantomData};

use ash::vk;
use type_kit::{
    Create, CreateResult, Destroy, DestroyResult, FromGuard, GenIndexRaw, TypeGuard, Valid,
};

use crate::context::{
    device::{
        memory::{MemoryProperties, MemoryTypeInfo},
        raw::allocator::{AllocationEntry, AllocationRequest, AllocatorIndex},
    },
    error::ResourceError,
    Context,
};

use super::{Resource, ResourceIndex};

pub trait ImageType: 'static {
    type Extent: Into<vk::Extent3D> + Clone + Copy + Debug;
    const IMAGE_TYPE: vk::ImageType;
    const VIEW_TYPE: vk::ImageViewType;
}

#[derive(Debug)]
pub struct Image2D;

impl ImageType for Image2D {
    type Extent = vk::Extent2D;
    const IMAGE_TYPE: vk::ImageType = vk::ImageType::TYPE_2D;
    const VIEW_TYPE: vk::ImageViewType = vk::ImageViewType::TYPE_2D;
}

#[derive(Debug)]
pub struct ImageCube;

impl ImageType for ImageCube {
    type Extent = vk::Extent2D;
    const IMAGE_TYPE: vk::ImageType = vk::ImageType::TYPE_2D;
    const VIEW_TYPE: vk::ImageViewType = vk::ImageViewType::CUBE;
}

#[derive(Debug, Clone, Copy)]
pub struct ImageInfo {
    pub extent: vk::Extent3D,
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub samples: vk::SampleCountFlags,
    pub aspect: vk::ImageAspectFlags,
}

#[derive(Debug, Clone, Copy)]
pub struct MipInfo {
    base_mip_level: u32,
    level_count: u32,
}

impl Default for MipInfo {
    #[inline]
    fn default() -> Self {
        Self {
            base_mip_level: 0,
            level_count: 1,
        }
    }
}

impl MipInfo {
    #[inline]
    fn get_max_for_extent(extent: vk::Extent3D) -> u32 {
        u32::max(extent.width, extent.height).ilog2() + 1
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ArrayInfo {
    base_array_layer: u32,
    layer_count: u32,
}

impl Default for ArrayInfo {
    #[inline]
    fn default() -> Self {
        Self {
            base_array_layer: 0,
            layer_count: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageCreateInfo<V: ImageType> {
    allocator: AllocatorIndex,
    memory_type_info: MemoryTypeInfo,
    image_info: ImageInfo,
    mip_info: Option<MipInfo>,
    array_info: Option<ArrayInfo>,
    _phantom: PhantomData<V>,
}

impl<V: ImageType> ImageCreateInfo<V> {
    pub fn with_mip_enabled(mut self) -> Self {
        let max_mip_levels = MipInfo::get_max_for_extent(self.image_info.extent);
        self.mip_info = Some(MipInfo {
            base_mip_level: 0,
            level_count: max_mip_levels,
        });
        self
    }

    pub fn with_array_layers(mut self, base_array_layer: u32, layer_count: u32) -> Self {
        self.array_info = Some(ArrayInfo {
            base_array_layer,
            layer_count,
        });
        self
    }

    fn get_image_create_info(&self) -> vk::ImageCreateInfo {
        let ImageInfo {
            extent,
            samples,
            format,
            usage,
            ..
        } = self.image_info;
        let ArrayInfo {
            layer_count: array_layers,
            ..
        } = self.array_info.unwrap_or_default();
        let MipInfo {
            level_count: mip_levels,
            ..
        } = self.mip_info.unwrap_or_default();
        vk::ImageCreateInfo {
            initial_layout: vk::ImageLayout::UNDEFINED,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            tiling: vk::ImageTiling::OPTIMAL,
            image_type: V::IMAGE_TYPE,
            samples,
            format,
            usage,
            extent,
            mip_levels,
            array_layers,
            ..Default::default()
        }
    }

    fn get_view_create_info(&self, image: vk::Image) -> vk::ImageViewCreateInfo {
        let ImageInfo {
            aspect: aspect_mask,
            format,
            ..
        } = self.image_info;
        let ArrayInfo {
            base_array_layer,
            layer_count,
        } = self.array_info.unwrap_or_default();
        let MipInfo {
            level_count,
            base_mip_level,
        } = self.mip_info.unwrap_or_default();
        vk::ImageViewCreateInfo {
            view_type: V::VIEW_TYPE,
            format,
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask,
                base_mip_level,
                level_count,
                base_array_layer,
                layer_count,
            },
            image,
            ..Default::default()
        }
    }

    fn get_allocation_request(&self, context: &Context, image: vk::Image) -> AllocationRequest {
        let requirements = unsafe { context.get_image_memory_requirements(image) };
        AllocationRequest::new(self.memory_type_info, requirements)
    }
}

#[derive(Debug)]
pub struct Image<V: ImageType> {
    handle: vk::Image,
    extent: vk::Extent3D,
    format: vk::Format,
    layout: vk::ImageLayout,
    usage: vk::ImageUsageFlags,
    view: ResourceIndex<ImageView<V>>,
    memory: AllocationEntry,
}

impl<V: ImageType> Image<V> {
    #[inline]
    pub fn create_info<M: MemoryProperties>(
        allocator: AllocatorIndex,
        image_info: ImageInfo,
    ) -> ImageCreateInfo<V> {
        ImageCreateInfo {
            allocator,
            memory_type_info: M::get_memory_type_info(),
            image_info,
            mip_info: None,
            array_info: None,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageRaw {
    handle: vk::Image,
    extent: vk::Extent3D,
    format: vk::Format,
    layout: vk::ImageLayout,
    usage: vk::ImageUsageFlags,
    view: TypeGuard<GenIndexRaw>,
    memory: AllocationEntry,
}

impl<V: ImageType> From<Valid<Image<V>>> for Image<V> {
    #[inline]
    fn from(guard: Valid<Image<V>>) -> Self {
        let inner = guard.into_inner();
        Self {
            handle: inner.handle,
            extent: inner.extent,
            format: inner.format,
            layout: inner.layout,
            usage: inner.usage,
            memory: inner.memory,
            view: {
                let view: Valid<ResourceIndex<ImageView<V>>> = inner.view.try_into().unwrap();
                view.into()
            },
        }
    }
}

impl<V: ImageType> FromGuard for Image<V> {
    type Inner = ImageRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        ImageRaw {
            handle: self.handle,
            extent: self.extent,
            format: self.format,
            layout: self.layout,
            usage: self.usage,
            memory: self.memory,
            view: self.view.into_guard(),
        }
    }
}

impl<V: ImageType> Resource for Image<V> {
    type RawType = ImageRaw;
}

impl<V: ImageType> Create for Image<V> {
    type Config<'a> = ImageCreateInfo<V>;
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let image_create_info = config.get_image_create_info();
        let handle = unsafe { context.create_image(&image_create_info, None)? };
        let view =
            context.create_resource::<ImageView<V>, _>(config.get_view_create_info(handle))?;
        let memory = context.allocate(
            config.allocator,
            config.get_allocation_request(&context, handle),
        )?;
        Ok(Self {
            handle,
            extent: config.image_info.extent.into(),
            format: config.image_info.format,
            usage: config.image_info.usage,
            layout: vk::ImageLayout::UNDEFINED,
            view,
            memory,
        })
    }
}

impl<V: ImageType> Destroy for Image<V> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_image(self.handle, None);
        }
        Ok(())
    }
}

impl Destroy for ImageRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_image(self.handle, None);
        }
        Ok(())
    }
}

pub struct ImageViewCreateInfo<'a> {
    pub format: vk::Format,
    pub aspect_mask: vk::ImageAspectFlags,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub view_type: vk::ImageViewType,
    pub image: vk::Image,
    pub components: vk::ComponentMapping,
    pub subresource_range: vk::ImageSubresourceRange,
    _phantom: PhantomData<&'a ()>,
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

impl<V: ImageType> From<Valid<ImageView<V>>> for ImageView<V> {
    #[inline]
    fn from(guard: Valid<ImageView<V>>) -> Self {
        let inner = guard.into_inner();
        Self {
            handle: inner.handle,
            _phantom: PhantomData,
        }
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
}

impl<V: ImageType> Resource for ImageView<V> {
    type RawType = ImageViewRaw;
}

impl<V: ImageType> Create for ImageView<V> {
    type Config<'a> = vk::ImageViewCreateInfo;
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let handle = unsafe { context.create_image_view(&config, None)? };
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
