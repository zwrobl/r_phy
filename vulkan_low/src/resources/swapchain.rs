use ash::vk;
use std::{convert::Infallible, fmt::Debug, marker::PhantomData, ptr::NonNull};
use type_kit::{
    Cons, Create, CreateCollection, CreateResult, Destroy, DestroyResult, DropGuard, FromGuard,
    GenCell, GenIndexRaw, TypeGuard, unpack_list,
};

use crate::{
    Context,
    error::ExtResult,
    index_list,
    resources::{
        Resource, ResourceGuardError, ResourceIndex,
        error::{GuardError, ResourceError, ResourceResult},
        framebuffer::{Extent2D, FramebufferBuilder, FramebufferRaw},
        image::{Image2D, ImageView, ImageViewCreateInfo},
        render_pass::RenderPassConfig,
        sync::Semaphore,
    },
    surface::PhysicalDeviceSurfaceProperties,
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

pub struct SwapchainFrame<C: RenderPassConfig> {
    pub framebuffer: FramebufferHandle<C>,
    pub render_area: vk::Rect2D,
    image_index: u32,
    draw_ready: Semaphore,
    draw_finished: Semaphore,
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
    pub semaphores: Option<NonNull<[Semaphore]>>,
    images: Option<NonNull<[SwapchainImage]>>,
    handle: vk::SwapchainKHR,
}

pub struct SwapchainPartial<C: RenderPassConfig> {
    extent: vk::Extent2D,
    images: Box<[SwapchainImage]>,
    handle: vk::SwapchainKHR,
    _phantom: PhantomData<C>,
}

impl<C: RenderPassConfig> SwapchainPartial<C> {
    #[inline]
    pub fn num_images(&self) -> usize {
        self.images.len()
    }
}

pub struct Swapchain<C: RenderPassConfig> {
    pub num_images: usize,
    pub extent: vk::Extent2D,
    pub framebuffers: Box<[GenIndexRaw]>,
    pub semaphores: Box<[Semaphore]>,
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
            error: Box::new((DropGuard::new(resource), err)),
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
            semaphores: NonNull::new(Box::leak(self.semaphores)),
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
            semaphores: unsafe { Box::from_raw(inner.semaphores.take().unwrap().as_mut()) },
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
        draw_ready: Semaphore,
    ) -> ResourceResult<SwapchainFrame<C>> {
        let (image_index, _) = unsafe {
            self.get_extensions().swapchain.acquire_next_image(
                swapchain.handle,
                u64::MAX,
                *draw_ready,
                vk::Fence::null(),
            )?
        };
        let framebuffer = self.get_framebuffer_handle(swapchain, image_index as usize)?;
        let draw_finished = swapchain.semaphores[image_index as usize];
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: swapchain.extent,
        };
        Ok(SwapchainFrame {
            framebuffer,
            render_area,
            image_index,
            draw_ready,
            draw_finished,
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
            draw_finished,
            draw_ready,
            ..
        } = frame;
        unsafe {
            self.submit_command(
                command,
                SubmitSemaphoreState {
                    semaphores: &[*draw_ready],
                    masks: &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT],
                },
                &[*draw_finished],
            )?;
            self.get_extensions().swapchain.queue_present(
                self.device_queues.graphics,
                &vk::PresentInfoKHR {
                    wait_semaphore_count: 1,
                    p_wait_semaphores: [*draw_finished].as_ptr(),
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

pub trait SwapchainFramebufferConfigBuilder<C: RenderPassConfig> {
    fn get_framebuffer_builder(
        &self,
        context: &Context,
        image_view: &ImageView<Image2D>,
        extent: Extent2D,
    ) -> FramebufferBuilder<C>;
}

impl<C: RenderPassConfig> Create for SwapchainPartial<C> {
    type Config<'a> = ();
    type CreateError = ResourceError;

    fn create<'a, 'b>(_config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
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
        Ok(SwapchainPartial {
            extent: image_extent,
            images,
            handle,
            _phantom: PhantomData,
        })
    }
}

impl<C: RenderPassConfig> Destroy for SwapchainPartial<C> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
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

impl<C: RenderPassConfig> Create for Swapchain<C> {
    type Config<'a> = (
        &'a dyn SwapchainFramebufferConfigBuilder<C>,
        SwapchainPartial<C>,
    );
    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (framebuffer_builder, swapchain_partial) = config;
        let SwapchainPartial {
            images,
            extent,
            handle,
            ..
        } = swapchain_partial;
        let framebuffers = images
            .iter()
            .map(|image| {
                context
                    .create_resource::<Framebuffer<C>, _>(
                        framebuffer_builder.get_framebuffer_builder(
                            context,
                            &image.view,
                            Extent2D::new(extent.width, extent.height),
                        ),
                    )
                    .map(|index| index.into_inner())
            })
            .collect::<Result<Box<_>, _>>()?;
        let semaphores = (0..images.len())
            .map(|_| ())
            .create(context)
            .collect::<Result<Box<_>, _>>()?;
        Ok(Swapchain {
            num_images: images.len(),
            extent,
            images,
            framebuffers,
            semaphores,
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
        self.semaphores.iter_mut().for_each(|semaphore| {
            let _ = semaphore.destroy(context);
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
        if let Some(mut semaphores) = self.semaphores.take() {
            let mut semaphores = unsafe { Box::from_raw(semaphores.as_mut()) };
            semaphores.iter_mut().for_each(|semaphore| {
                let _ = semaphore.destroy(context);
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
