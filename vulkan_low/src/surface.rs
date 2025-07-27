use ash::{self, extensions::khr, vk};
use std::{
    collections::HashSet,
    convert::Infallible,
    ffi::{c_void, CStr},
    ptr::null,
};
use type_kit::{Create, CreateResult, Destroy, DestroyResult};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle, Win32WindowHandle},
    window::Window,
};

use super::error::{DeviceNotSuitable, VkError, VkResult};
use super::Instance;

pub struct Surface {
    handle: vk::SurfaceKHR,
    loader: khr::Surface,
}

#[cfg(target_os = "windows")]
fn create_platform_surface(instance: &Instance, window: &Window) -> VkResult<vk::SurfaceKHR> {
    let win32_surface: khr::Win32Surface = instance.load();
    let (hwnd, hinstance) = match window.window_handle()?.as_raw() {
        RawWindowHandle::Win32(Win32WindowHandle {
            hwnd, hinstance, ..
        }) => {
            let hwnd = hwnd.get() as *const c_void;
            let hinstance = hinstance.map_or(null(), |hinstance| hinstance.get() as *const c_void);
            (hwnd, hinstance)
        }
        _ => panic!("Unexpected RawWindowHandleType for current platform!"),
    };
    let handle = unsafe {
        win32_surface.create_win32_surface(
            &vk::Win32SurfaceCreateInfoKHR::builder()
                .hwnd(hwnd)
                .hinstance(hinstance),
            None,
        )?
    };
    Ok(handle)
}

#[cfg(not(target_os = "windows"))]
fn create_platform_surface(
    entry: &Entry,
    instance: &Instance,
    window: &Window,
) -> VkResult<vk::SurfaceKHR> {
    compile_error!("Current platform not supported!");
}

impl Surface {
    #[cfg(target_os = "windows")]
    pub fn iterate_required_extensions() -> impl Iterator<Item = &'static CStr> {
        const REQUIRED_EXTENSIONS: [&CStr; 2] = [khr::Win32Surface::name(), khr::Surface::name()];
        REQUIRED_EXTENSIONS.into_iter()
    }

    #[cfg(not(target_os = "windows"))]
    pub fn iterate_required_extensions() -> impl Iterator<Item = &'static CStr> {
        compile_error!("Current platform not supported!");
    }
}

impl Create for Surface {
    type Config<'a> = &'a Window;
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let handle = create_platform_surface(context, config)?;
        let loader: khr::Surface = context.load();
        Ok(Self { handle, loader })
    }
}

impl Destroy for Surface {
    type Context<'a> = &'a Instance;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe { self.loader.destroy_surface(self.handle, None) };
        Ok(())
    }
}

impl From<&Surface> for vk::SurfaceKHR {
    fn from(value: &Surface) -> Self {
        value.handle
    }
}

#[derive(Debug, Clone)]
pub struct PhysicalDeviceSurfaceProperties {
    pub present_mode: vk::PresentModeKHR,
    pub surface_format: vk::SurfaceFormatKHR,
    pub supported_queue_families: HashSet<u32>,
    pub capabilities: vk::SurfaceCapabilitiesKHR,
}

impl PhysicalDeviceSurfaceProperties {
    const PREFERRED_SURFACE_FORMATS: &'static [vk::Format] =
        &[vk::Format::R8G8B8A8_SRGB, vk::Format::B8G8R8A8_SRGB];

    pub fn get(
        surface: &Surface,
        physical_device: vk::PhysicalDevice,
        quque_families: &[(vk::QueueFamilyProperties, u32)],
    ) -> Result<Self, DeviceNotSuitable> {
        let surface_formats = unsafe {
            surface
                .loader
                .get_physical_device_surface_formats(physical_device, surface.handle)?
        };
        let surface_format = *Self::PREFERRED_SURFACE_FORMATS
            .iter()
            .find_map(|&pref| {
                surface_formats.iter().find(|supported| {
                    supported.format == pref
                        && supported.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                })
            })
            .or(surface_formats.first())
            .ok_or(DeviceNotSuitable::MissingSurfaceSupport)?;
        let present_mode = unsafe {
            surface
                .loader
                .get_physical_device_surface_present_modes(physical_device, surface.handle)?
        }
        .into_iter()
        .find(|&present_mode| present_mode == vk::PresentModeKHR::MAILBOX)
        .unwrap_or(vk::PresentModeKHR::FIFO);
        let supported_queue_families = HashSet::<u32>::from_iter(
            quque_families
                .iter()
                .filter(|&&(properties, queue_family_index)| {
                    properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        && unsafe {
                            surface
                                .loader
                                .get_physical_device_surface_support(
                                    physical_device,
                                    queue_family_index,
                                    surface.handle,
                                )
                                .unwrap_or(false)
                        }
                })
                .map(|&(_, queue_family_index)| queue_family_index),
        );
        if supported_queue_families.is_empty() {
            Err(DeviceNotSuitable::MissingSurfaceSupport)?;
        }
        let capabilities = unsafe {
            surface
                .loader
                .get_physical_device_surface_capabilities(physical_device, surface.handle)?
        };
        Ok(Self {
            present_mode,
            surface_format,
            supported_queue_families,
            capabilities,
        })
    }

    pub fn get_current_extent(&self) -> vk::Extent2D {
        let vk::SurfaceCapabilitiesKHR {
            current_extent,
            min_image_extent,
            max_image_extent,
            ..
        } = self.capabilities;
        vk::Extent2D {
            width: current_extent
                .width
                .clamp(min_image_extent.width, max_image_extent.width),
            height: current_extent
                .height
                .clamp(min_image_extent.height, max_image_extent.height),
        }
    }

    pub fn get_image_count(&self) -> u32 {
        let vk::SurfaceCapabilitiesKHR {
            min_image_count,
            max_image_count,
            ..
        } = self.capabilities;
        (min_image_count + 1).clamp(
            0,
            match max_image_count {
                0 => u32::MAX,
                _ => max_image_count,
            },
        )
    }
}
