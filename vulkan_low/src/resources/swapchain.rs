use ash::vk;
use std::{convert::Infallible, fmt::Debug, marker::PhantomData, ptr::NonNull};
use type_kit::{
    unpack_list, Cons, Create, CreateResult, Destroy, DestroyResult, DropGuard, FromGuard, GenCell,
    GenIndexRaw, TypeGuard,
};

use crate::{
    error::ExtResult,
    index_list,
    resources::{
        error::{GuardError, ResourceError, ResourceResult},
        framebuffer::{Extent2D, FramebufferBuilder, FramebufferRaw},
        image::{Image2D, ImageView, ImageViewCreateInfo},
        render_pass::RenderPassConfig,
        Resource, ResourceGuardError, ResourceIndex,
    },
    surface::PhysicalDeviceSurfaceProperties,
    Context,
};

use crate::{
    device::Device,
    resources::{
        command::{
            FinishedCommand, Graphics, Operation, Persistent, Primary, SubmitSemaphoreState,
        },
        framebuffer::{Framebuffer, FramebufferHandle},
        storage::ResourceIndexListBuilder,
    },
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
    view: ImageView<Image2D>,
}

#[derive(Debug, Clone, Copy)]
pub struct SwapchainRaw {
    pub num_images: usize,
    pub extent: vk::Extent2D,
    pub framebuffers: Option<NonNull<[GenIndexRaw]>>,
    images: Option<NonNull<[SwapchainImage]>>,
    handle: vk::SwapchainKHR,
}

pub struct Swapchain<C: RenderPassConfig> {
    pub num_images: usize,
    pub extent: vk::Extent2D,
    pub framebuffers: Box<[GenIndexRaw]>,
    images: Box<[SwapchainImage]>,
    handle: vk::SwapchainKHR,
    _phantom: PhantomData<C>,
}

impl<C: RenderPassConfig> Resource for Swapchain<C> {
    type RawType = SwapchainRaw;
    type RawCollection = GenCell<TypeGuard<Self::RawType>>;

    #[inline]
    fn wrap_guard_error((resource, err): ResourceGuardError<Self>) -> ResourceError {
        ResourceError::GuardError(GuardError::Swapchain {
            error: (DropGuard::new(resource), err),
        })
    }
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

impl Context {
    #[inline]
    pub fn get_framebuffer_handle<C: RenderPassConfig>(
        &self,
        swapchain: &Swapchain<C>,
        index: usize,
    ) -> ResourceResult<FramebufferHandle<C>> {
        let handle = self.operate_ref(
            index_list![swapchain.get_framebuffer_index(index)],
            |unpack_list![framebuffer]| framebuffer.into(),
        )?;
        Ok(handle)
    }

    pub fn get_frame<C: RenderPassConfig>(
        &self,
        swapchain: &Swapchain<C>,
        image_sync: SwapchainImageSync,
    ) -> ResourceResult<SwapchainFrame<C>> {
        let (image_index, _) = unsafe {
            self.get_extensions().swapchain.acquire_next_image(
                swapchain.handle,
                u64::MAX,
                image_sync.draw_ready,
                vk::Fence::null(),
            )?
        };
        let framebuffer = self.get_framebuffer_handle(swapchain, image_index as usize)?;
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
    ) -> ExtResult<()> {
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
            self.get_extensions().swapchain.queue_present(
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
        let view = ImageView::create(
            ImageViewCreateInfo::new()
                .with_image(image)
                .with_format(surface_format.format)
                .with_aspect(vk::ImageAspectFlags::COLOR),
            self,
        )?;
        Ok(SwapchainImage {
            _image: image,
            view,
        })
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

pub trait SwapchainFramebufferConfigBuilder<C: RenderPassConfig> {
    fn get_framebuffer_builder(
        &self,
        context: &Context,
        image_view: &ImageView<Image2D>,
        extent: Extent2D,
    ) -> FramebufferBuilder<C>;
}

impl<C: RenderPassConfig> Create for Swapchain<C> {
    type Config<'a> = &'a dyn SwapchainFramebufferConfigBuilder<C>;
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
        let laoder = &context.get_extensions().swapchain;
        let handle = unsafe { laoder.create_swapchain(&create_info, None)? };
        let images = unsafe {
            laoder
                .get_swapchain_images(handle)?
                .into_iter()
                .map(|image| context.create_swapchain_image(image, surface_format))
                .collect::<Result<Box<_>, _>>()?
        };
        let framebuffers = images
            .iter()
            .map(|image| {
                context
                    .create_resource::<Framebuffer<C>, _>(config.get_framebuffer_builder(
                        context,
                        &image.view,
                        Extent2D::new(image_extent.width, image_extent.height),
                    ))
                    .map(|index| index.into_inner())
            })
            .collect::<Result<Box<_>, _>>()?;
        Ok(Swapchain {
            num_images: images.len(),
            extent: image_extent,
            images,
            framebuffers,
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
        self.images.iter_mut().for_each(|image| {
            let _ = image.view.destroy(context);
        });
        unsafe {
            context
                .get_extensions()
                .swapchain
                .destroy_swapchain(self.handle, None);
        }
        Ok(())
    }
}

impl Destroy for SwapchainRaw {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        if let Some(mut framebuffers) = self.framebuffers.take() {
            let framebuffers = unsafe { Box::from_raw(framebuffers.as_mut()) };
            (0..framebuffers.len()).for_each(|index| {
                let _ = unsafe {
                    context.destroy_raw_resource::<FramebufferRaw, _>(framebuffers[index])
                };
            })
        };
        if let Some(mut images) = self.images.take() {
            let mut images = unsafe { Box::from_raw(images.as_mut()) };
            images.iter_mut().for_each(|image| {
                let _ = image.view.destroy(context);
            })
        };
        unsafe {
            context
                .get_extensions()
                .swapchain
                .destroy_swapchain(self.handle, None)
        };
        Ok(())
    }
}
