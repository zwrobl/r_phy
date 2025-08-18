mod sampler;
mod texture;
mod view;

pub use sampler::*;
pub use texture::*;
pub use view::*;

use std::{convert::Infallible, fmt::Debug, marker::PhantomData};

use ash::vk;

use crate::{
    Context,
    memory::{
        AllocReqTyped, BindResource, DeviceLocal, MemoryProperties,
        allocator::{AllocationEntry, AllocationEntryTyped, AllocatorBuilder, AllocatorIndex},
    },
    resources::{
        Partial, ResourceGuardError,
        error::{GuardError, ResourceError, ResourceResult},
    },
};
use type_kit::{Create, CreateResult, Destroy, DestroyResult, DropGuard, FromGuard, GuardVec};

use super::Resource;

pub trait ImageType: 'static {
    type Extent: Into<vk::Extent3D> + Clone + Copy + Debug;
    type ImageReader<'a>: ImageReader<Type = Self>;
    const IMAGE_FLAGS: vk::ImageCreateFlags;
    const IMAGE_TYPE: vk::ImageType;
    const VIEW_TYPE: vk::ImageViewType;
}

#[derive(Debug)]
pub struct Image2D;

impl ImageType for Image2D {
    type Extent = vk::Extent2D;
    type ImageReader<'a> = Image2DReader<'a>;
    const IMAGE_FLAGS: vk::ImageCreateFlags = vk::ImageCreateFlags::empty();
    const IMAGE_TYPE: vk::ImageType = vk::ImageType::TYPE_2D;
    const VIEW_TYPE: vk::ImageViewType = vk::ImageViewType::TYPE_2D;
}

#[derive(Debug)]
pub struct ImageCube;

impl ImageType for ImageCube {
    type Extent = vk::Extent2D;
    type ImageReader<'a> = ImageCubeReader;
    const IMAGE_FLAGS: vk::ImageCreateFlags = vk::ImageCreateFlags::CUBE_COMPATIBLE;
    const IMAGE_TYPE: vk::ImageType = vk::ImageType::TYPE_2D;
    const VIEW_TYPE: vk::ImageViewType = vk::ImageViewType::CUBE;
}

#[derive(Debug, Clone, Copy)]
pub struct MipInfo {
    pub base_mip_level: u32,
    pub level_count: u32,
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
    pub base_array_layer: u32,
    pub layer_count: u32,
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

#[derive(Debug, Clone, Copy, Default)]
pub struct ImageInfo {
    pub extent: vk::Extent3D,
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub samples: vk::SampleCountFlags,
    pub aspect: vk::ImageAspectFlags,
    pub mip_info: Option<MipInfo>,
    pub array_info: Option<ArrayInfo>,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageCreateInfo<V: ImageType, M: MemoryProperties> {
    image_info: ImageInfo,
    _phantom: PhantomData<(V, M)>,
}

impl<V: ImageType, M: MemoryProperties> ImageCreateInfo<V, M> {
    pub fn new(image_info: ImageInfo) -> Self {
        Self {
            image_info,
            _phantom: PhantomData,
        }
    }

    pub fn with_mip_enabled(mut self) -> Self {
        let max_mip_levels = MipInfo::get_max_for_extent(self.image_info.extent);
        self.image_info.mip_info = Some(MipInfo {
            base_mip_level: 0,
            level_count: max_mip_levels,
        });
        self
    }

    pub fn with_array_layers(mut self, base_array_layer: u32, layer_count: u32) -> Self {
        self.image_info.array_info = Some(ArrayInfo {
            base_array_layer,
            layer_count,
        });
        self
    }

    fn get_vk_create_info(&self) -> vk::ImageCreateInfo {
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
        } = self.image_info.array_info.unwrap_or_default();
        let MipInfo {
            level_count: mip_levels,
            ..
        } = self.image_info.mip_info.unwrap_or_default();
        vk::ImageCreateInfo {
            initial_layout: vk::ImageLayout::UNDEFINED,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            tiling: vk::ImageTiling::OPTIMAL,
            image_type: V::IMAGE_TYPE,
            flags: V::IMAGE_FLAGS,
            samples,
            format,
            usage,
            extent,
            mip_levels,
            array_layers,
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub struct ImagePartial<V: ImageType, M: MemoryProperties> {
    image: vk::Image,
    alloc_req: AllocReqTyped<M>,
    create_info: ImageCreateInfo<V, M>,
}

impl<V: ImageType, M: MemoryProperties> Create for ImagePartial<V, M> {
    type Config<'a> = ImageCreateInfo<V, M>;

    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let image = unsafe { context.create_image(&config.get_vk_create_info(), None)? };
        let alloc_req = BindResource::new(image).get_alloc_req(context);
        Ok(Self {
            image,
            alloc_req,
            create_info: config,
        })
    }
}

impl<V: ImageType, M: MemoryProperties> Partial for ImagePartial<V, M> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        builder.with_allocation(self.alloc_req);
    }
}

impl<V: ImageType, M: MemoryProperties> Destroy for ImagePartial<V, M> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_image(self.image, None);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Image<V: ImageType, M: MemoryProperties> {
    image: vk::Image,
    view: ImageView<V>,
    layout: vk::ImageLayout,
    allocation: AllocationEntryTyped<M>,
    image_info: ImageInfo,
}

impl<V: ImageType, M: MemoryProperties> Image<V, M> {
    #[inline]
    pub fn get_vk_image(&self) -> vk::Image {
        self.image
    }

    #[inline]
    pub fn get_vk_layout(&self) -> vk::ImageLayout {
        self.layout
    }

    #[inline]
    pub fn set_vk_layout(&mut self, layout: vk::ImageLayout) {
        self.layout = layout
    }

    #[inline]
    pub fn get_image_view(&self) -> &ImageView<V> {
        &self.view
    }

    #[inline]
    pub fn get_image_info(&self) -> &ImageInfo {
        &self.image_info
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageRaw {
    image: vk::Image,
    view: ImageViewRaw,
    layout: vk::ImageLayout,
    allocation: AllocationEntry,
    image_info: ImageInfo,
}

impl<V: ImageType, M: MemoryProperties> FromGuard for Image<V, M> {
    type Inner = ImageRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        ImageRaw {
            image: self.image,
            image_info: self.image_info,
            layout: self.layout,
            allocation: self.allocation.into_inner(),
            view: self.view.into_inner(),
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            image: inner.image,
            image_info: inner.image_info,
            layout: inner.layout,
            allocation: unsafe { AllocationEntryTyped::from_inner(inner.allocation) },
            view: unsafe { ImageView::from_inner(inner.view) },
        }
    }
}

impl<V: ImageType, M: MemoryProperties> Resource for Image<V, M> {
    type RawType = ImageRaw;
    type RawCollection = GuardVec<Self::RawType>;

    #[inline]
    fn wrap_guard_error((resource, err): ResourceGuardError<Self>) -> ResourceError {
        ResourceError::GuardError(GuardError::Image {
            error: Box::new((DropGuard::new(resource), err)),
        })
    }
}

impl<V: ImageType, M: MemoryProperties> Create for Image<V, M> {
    type Config<'a> = (DropGuard<ImagePartial<V, M>>, Option<AllocatorIndex>);
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (image_partial, allocator) = config;
        let ImagePartial {
            image,
            alloc_req,
            create_info,
        } = unsafe { image_partial.unwrap() };
        let allocation = context.allocate(alloc_req, allocator)?;
        context.bind_memory(image, allocation)?;
        let view = ImageView::create(create_info.get_view_create_info(image), context)?;
        Ok(Self {
            image,
            image_info: create_info.image_info,
            layout: vk::ImageLayout::UNDEFINED,
            view,
            allocation,
        })
    }
}

impl<V: ImageType, M: MemoryProperties> Destroy for Image<V, M> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.view.destroy(context);
        unsafe {
            context.destroy_image(self.image, None);
        }
        let _ = context.free(self.allocation);
        Ok(())
    }
}

impl Destroy for ImageRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.view.destroy(context);
        unsafe {
            context.destroy_image(self.image, None);
        }
        let _ = context.free_allocation_raw(self.allocation);
        Ok(())
    }
}

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
