use ash::{extensions::khr, vk};
use std::{convert::Infallible, error::Error, ffi::CStr};
use type_kit::{Create, CreateResult, Destroy, DestroyResult};

use crate::context::{
    device::raw::resources::render_pass::RenderPassConfig,
    error::{ResourceResult, VkError, VkResult},
    surface::PhysicalDeviceSurfaceProperties,
    Context,
};

use super::{
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

pub struct Swapchain<C: RenderPassConfig> {
    pub num_images: usize,
    pub extent: vk::Extent2D,
    pub framebuffers: Vec<Framebuffer<C>>,
    images: Vec<SwapchainImage>,
    handle: vk::SwapchainKHR,
    loader: khr::Swapchain,
}

pub const fn required_extensions() -> &'static [&'static CStr; 1] {
    const REQUIRED_DEVICE_EXTENSIONS: &[&CStr; 1] = &[khr::Swapchain::name()];
    REQUIRED_DEVICE_EXTENSIONS
}

impl<C: RenderPassConfig> Swapchain<C> {
    pub fn get_frame(
        &self,
        image_sync: SwapchainImageSync,
    ) -> Result<SwapchainFrame<C>, Box<dyn Error>> {
        let (image_index, _) = unsafe {
            self.loader.acquire_next_image(
                self.handle,
                u64::MAX,
                image_sync.draw_ready,
                vk::Fence::null(),
            )?
        };
        let framebuffer = (&self.framebuffers[image_index as usize]).into();
        let render_area = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: self.extent,
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
    ) -> VkResult<SwapchainImage> {
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
    type CreateError = VkError;

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
    type Context<'a> = &'a Device;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_semaphore(self.draw_ready, None);
            context.destroy_semaphore(self.draw_finished, None);
        }
        Ok(())
    }
}

pub trait FramebufferBuilder<C: RenderPassConfig> {
    fn build(
        &self,
        image_view: vk::ImageView,
        extent: vk::Extent2D,
    ) -> ResourceResult<Framebuffer<C>>;
}

impl<C: RenderPassConfig, F> FramebufferBuilder<C> for F
where
    F: Fn(vk::ImageView, vk::Extent2D) -> ResourceResult<Framebuffer<C>>,
{
    #[inline]
    fn build(
        &self,
        image_view: vk::ImageView,
        extent: vk::Extent2D,
    ) -> ResourceResult<Framebuffer<C>> {
        self(image_view, extent)
    }
}

impl<C: RenderPassConfig> Create for Swapchain<C> {
    type Config<'a> = &'a dyn FramebufferBuilder<C>;
    type CreateError = VkError;

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
                .collect::<Result<Vec<_>, _>>()?
        };
        let framebuffers = images
            .iter()
            .map(|image| config.build(image.view, image_extent))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Swapchain {
            num_images: images.len(),
            extent: image_extent,
            images,
            framebuffers,
            loader,
            handle,
        })
    }
}

impl<C: RenderPassConfig> Destroy for Swapchain<C> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.framebuffers.iter_mut().for_each(|framebuffer| {
            let _ = framebuffer.destroy(context);
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
