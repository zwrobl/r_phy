use ash::{extensions::khr, vk};
use std::{
    convert::Infallible, error::Error, ffi::CStr, fmt::Debug, marker::PhantomData, ops::Deref,
    ptr::NonNull,
};
use type_kit::{Create, CreateResult, Destroy, DestroyResult, FromGuard, GenIndexRaw};

use crate::{
    device::raw::resources::{
        framebuffer::{FramebufferBuilder, FramebufferRaw},
        render_pass::RenderPassConfig,
        Resource, ResourceIndex,
    },
    error::{ResourceError, ResourceResult},
    surface::PhysicalDeviceSurfaceProperties,
    Context,
};

use crate::device::{
    raw::resources::command::{
        level::Primary,
        operation::{Graphics, Operation},
        FinishedCommand, Persistent, SubmitSemaphoreState,
    },
    raw::resources::framebuffer::{Framebuffer, FramebufferHandle},
    Device,
};
#[derive(Debug, Clone, Copy)]
pub struct SwapchainImageSync {
    draw_ready: vk::Semaphore,
    draw_finished: vk::Semaphore,
}

pub struct SwapchainFrame<C: RenderPassConfig> {
    pub framebuffer: FramebufferHandle<C>,
    pub render_area: vk::Rect2D,
    image_index: u32,
    image_sync: SwapchainImageSync,
}

struct SwapchainImage {
    _image: vk::Image,
    view: vk::ImageView,
}

#[derive(Clone, Copy)]
pub struct SwapchainRaw {
    pub num_images: usize,
    pub extent: vk::Extent2D,
    pub framebuffers: Option<NonNull<[GenIndexRaw]>>,
    images: Option<NonNull<[SwapchainImage]>>,
    loader: Option<NonNull<khr::Swapchain>>,
    handle: vk::SwapchainKHR,
}

pub struct Swapchain<C: RenderPassConfig> {
    pub num_images: usize,
    pub extent: vk::Extent2D,
    pub framebuffers: Box<[GenIndexRaw]>,
    images: Box<[SwapchainImage]>,
    loader: Box<khr::Swapchain>,
    handle: vk::SwapchainKHR,
    _phantom: PhantomData<C>,
}

impl<C: RenderPassConfig> Resource for Swapchain<C> {
    type RawType = SwapchainRaw;
}

impl<C: RenderPassConfig> FromGuard for Swapchain<C> {
    type Inner = SwapchainRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner {
            num_images: self.num_images,
            extent: self.extent,
            framebuffers: NonNull::new(Box::leak(self.framebuffers)),
            images: NonNull::new(Box::leak(self.images)),
            loader: NonNull::new(Box::leak(self.loader)),
            handle: self.handle,
        }
    }

    #[inline]
    unsafe fn from_inner(mut inner: Self::Inner) -> Self {
        Self {
            num_images: inner.num_images,
            extent: inner.extent,
            framebuffers: unsafe { Box::from_raw(inner.framebuffers.take().unwrap().as_mut()) },
            images: unsafe { Box::from_raw(inner.images.take().unwrap().as_mut()) },
            loader: unsafe { Box::from_raw(inner.loader.take().unwrap().as_mut()) },
            handle: inner.handle,
            _phantom: PhantomData,
        }
    }
}

impl<C: RenderPassConfig> Swapchain<C> {
    #[inline]
    pub fn get_framebuffer_index(&self, index: usize) -> ResourceIndex<Framebuffer<C>> {
        unsafe { ResourceIndex::<Framebuffer<C>>::from_inner(self.framebuffers[index]) }
    }
}

pub const fn required_extensions() -> &'static [&'static CStr; 1] {
    const REQUIRED_DEVICE_EXTENSIONS: &[&CStr; 1] = &[khr::Swapchain::name()];
    REQUIRED_DEVICE_EXTENSIONS
}

impl Context {
    #[inline]
    pub fn get_framebuffer_handle<C: RenderPassConfig>(
        &self,
        swapchain: &Swapchain<C>,
        index: usize,
    ) -> FramebufferHandle<C> {
        self.storage
            .borrow()
            .entry(swapchain.get_framebuffer_index(index))
            .unwrap()
            .deref()
            .into()
    }

    pub fn get_frame<C: RenderPassConfig>(
        &self,
        swapchain: &Swapchain<C>,
        image_sync: SwapchainImageSync,
    ) -> Result<SwapchainFrame<C>, Box<dyn Error>> {
        let (image_index, _) = unsafe {
            swapchain.loader.acquire_next_image(
                swapchain.handle,
                u64::MAX,
                image_sync.draw_ready,
                vk::Fence::null(),
            )?
        };
        let framebuffer = self.get_framebuffer_handle(swapchain, image_index as usize);
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: swapchain.extent,
        };
        Ok(SwapchainFrame {
            framebuffer,
            render_area,
            image_index,
            image_sync,
        })
    }
}

impl Device {
    pub fn present_frame<C: RenderPassConfig>(
        &self,
        swapchain: &Swapchain<C>,
        command: FinishedCommand<Persistent, Primary, Graphics>,
        frame: SwapchainFrame<C>,
    ) -> Result<(), Box<dyn Error>> {
        let SwapchainFrame {
            image_index,
            image_sync,
            ..
        } = frame;
        unsafe {
            self.submit_command(
                command,
                SubmitSemaphoreState {
                    semaphores: &[image_sync.draw_ready],
                    masks: &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT],
                },
                &[image_sync.draw_finished],
            )?;
            swapchain.loader.queue_present(
                self.device_queues.graphics,
                &vk::PresentInfoKHR {
                    wait_semaphore_count: 1,
                    p_wait_semaphores: [image_sync.draw_finished].as_ptr(),
                    swapchain_count: 1,
                    p_swapchains: [swapchain.handle].as_ptr(),
                    p_image_indices: [image_index].as_ptr(),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    }
}

impl Context {
    fn create_swapchain_image(
        &self,
        image: vk::Image,
        surface_format: vk::SurfaceFormatKHR,
    ) -> ResourceResult<SwapchainImage> {
        unsafe {
            let view = self.device.create_image_view(
                &vk::ImageViewCreateInfo::builder()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(surface_format.format)
                    .components(vk::ComponentMapping::default())
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )?;

            Ok(SwapchainImage {
                _image: image,
                view,
            })
        }
    }
}

impl Create for SwapchainImageSync {
    type Config<'a> = ();
    type CreateError = ResourceError;

    fn create<'a, 'b>(_: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let create_info = vk::SemaphoreCreateInfo::default();
        unsafe {
            let draw_ready = context.device.create_semaphore(&create_info, None)?;
            let draw_finished = context.device.create_semaphore(&create_info, None)?;
            Ok(SwapchainImageSync {
                draw_ready,
                draw_finished,
            })
        }
    }
}

impl Destroy for SwapchainImageSync {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_semaphore(self.draw_ready, None);
            context.destroy_semaphore(self.draw_finished, None);
        }
        Ok(())
    }
}

pub trait FramebufferConfigBuilder<C: RenderPassConfig> {
    fn get(&self, image_view: vk::ImageView, extent: vk::Extent2D) -> FramebufferBuilder<C>;
}

impl<C: RenderPassConfig, F> FramebufferConfigBuilder<C> for F
where
    F: Fn(vk::ImageView, vk::Extent2D) -> FramebufferBuilder<C>,
{
    #[inline]
    fn get(&self, image_view: vk::ImageView, extent: vk::Extent2D) -> FramebufferBuilder<C> {
        self(image_view, extent)
    }
}

impl<C: RenderPassConfig> Create for Swapchain<C> {
    type Config<'a> = &'a dyn FramebufferConfigBuilder<C>;
    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let surface_properties = &context.physical_device.surface_properties;
        let &PhysicalDeviceSurfaceProperties {
            capabilities:
                vk::SurfaceCapabilitiesKHR {
                    current_transform, ..
                },
            surface_format,
            present_mode,
            ..
        } = surface_properties;
        let min_image_count = surface_properties.get_image_count();
        let image_extent = surface_properties.get_current_extent();
        let queue_family_indices = [Graphics::get_queue_family_index(context)];
        let create_info = vk::SwapchainCreateInfoKHR::builder()
            .pre_transform(current_transform)
            .image_extent(image_extent)
            .min_image_count(min_image_count)
            .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
            .image_format(surface_format.format)
            .image_color_space(surface_format.color_space)
            .present_mode(present_mode)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .queue_family_indices(&queue_family_indices)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .clipped(true)
            .image_array_layers(1)
            .surface((&*context.surface).into());
        let loader: khr::Swapchain = context.load();
        let handle = unsafe { loader.create_swapchain(&create_info, None)? };
        let images = unsafe {
            loader
                .get_swapchain_images(handle)?
                .into_iter()
                .map(|image| context.create_swapchain_image(image, surface_format))
                .collect::<Result<Box<_>, _>>()?
        };
        let framebuffers = images
            .iter()
            .map(|image| {
                context
                    .create_resource::<Framebuffer<C>, _>(config.get(image.view, image_extent))
                    .map(|index| index.into_inner())
            })
            .collect::<Result<Box<_>, _>>()?;
        Ok(Swapchain {
            num_images: images.len(),
            extent: image_extent,
            images,
            framebuffers,
            loader: Box::new(loader),
            handle,
            _phantom: PhantomData,
        })
    }
}

impl<C: RenderPassConfig> Destroy for Swapchain<C> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        (0..self.framebuffers.len()).for_each(|index| {
            let _ = context.destroy_resource(self.get_framebuffer_index(index));
        });
        unsafe {
            self.images
                .iter_mut()
                .for_each(|image| context.destroy_image_view(image.view, None));
            self.loader.destroy_swapchain(self.handle, None);
        }
        Ok(())
    }
}

impl Destroy for SwapchainRaw {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.framebuffers.take().map(|mut framebuffers| {
            let framebuffers = unsafe { Box::from_raw(framebuffers.as_mut()) };
            (0..framebuffers.len()).for_each(|index| {
                let _ = unsafe {
                    context.destroy_raw_resource::<FramebufferRaw, _>(framebuffers[index])
                };
            })
        });
        self.images.take().map(|mut images| {
            let images = unsafe { Box::from_raw(images.as_mut()) };
            images
                .iter()
                .for_each(|image| unsafe { context.destroy_image_view(image.view, None) })
        });
        self.loader.take().map(|mut loader| {
            let loader = unsafe { Box::from_raw(loader.as_mut()) };
            unsafe { loader.destroy_swapchain(self.handle, None) }
        });
        Ok(())
    }
}
