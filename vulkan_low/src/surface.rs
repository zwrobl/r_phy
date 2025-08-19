use ash::{self, extensions::khr, vk};
use std::{
    collections::HashSet,
    convert::Infallible,
    ffi::{CStr, c_void},
    ptr::null,
};
use type_kit::{Create, CreateResult, Destroy, DestroyResult};
use winit::{
    dpi::PhysicalSize,
    raw_window_handle::{
        HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, Win32WindowHandle,
    },
    window::Window,
};

use super::Instance;
use crate::{
    device::error::{PhysicalDeviceError, PhysicalDeviceResult},
    error::{ExtError, ExtResult},
};

pub struct Surface {
    handle: vk::SurfaceKHR,
    loader: khr::Surface,
    extent: vk::Extent2D,
}

fn create_platform_surface(instance: &Instance, window: &Window) -> ExtResult<vk::SurfaceKHR> {
    let window_handle = window.window_handle()?.as_raw();
    let display_handle = window.display_handle()?.as_raw();
    let handle = match (display_handle, window_handle) {
        (
            RawDisplayHandle::Windows(_),
            RawWindowHandle::Win32(Win32WindowHandle {
                hwnd, hinstance, ..
            }),
        ) => {
            let win32_surface: khr::Win32Surface = instance.load();
            let hwnd = hwnd.get() as *const c_void;
            let hinstance = hinstance.map_or(null(), |hinstance| hinstance.get() as *const c_void);
            unsafe {
                win32_surface.create_win32_surface(
                    &vk::Win32SurfaceCreateInfoKHR::builder()
                        .hwnd(hwnd)
                        .hinstance(hinstance),
                    None,
                )?
            }
        }
        (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(window)) => {
            let wayland_surface: khr::WaylandSurface = instance.load();
            unsafe {
                wayland_surface.create_wayland_surface(
                    &vk::WaylandSurfaceCreateInfoKHR::builder()
                        .display(display.display.as_ptr())
                        .surface(window.surface.as_ptr()),
                    None,
                )?
            }
        }
        (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(window)) => {
            let xlib_surface: khr::XlibSurface = instance.load();
            unsafe {
                xlib_surface.create_xlib_surface(
                    &vk::XlibSurfaceCreateInfoKHR::builder()
                        .dpy(display.display.unwrap().as_ptr() as *mut _)
                        .window(window.window),
                    None,
                )?
            }
        }
        _ => panic!("Unexpected RawWindowHandleType for current platform!"),
    };
    Ok(handle)
}

impl Surface {
    pub fn iterate_required_extensions(
        window: &Window,
    ) -> ExtResult<impl Iterator<Item = &'static CStr>> {
        match window.window_handle()?.as_raw() {
            RawWindowHandle::Win32(_) => {
                const REQUIRED_EXTENSIONS: [&CStr; 2] =
                    [khr::Win32Surface::name(), khr::Surface::name()];
                Ok(REQUIRED_EXTENSIONS.into_iter())
            }
            RawWindowHandle::Wayland(_) => {
                const REQUIRED_EXTENSIONS: [&CStr; 2] =
                    [khr::WaylandSurface::name(), khr::Surface::name()];
                Ok(REQUIRED_EXTENSIONS.into_iter())
            }
            RawWindowHandle::Xlib(_) => {
                const REQUIRED_EXTENSIONS: [&CStr; 2] =
                    [khr::XlibSurface::name(), khr::Surface::name()];
                Ok(REQUIRED_EXTENSIONS.into_iter())
            }
            _ => panic!("Unsupported platform!"),
        }
    }
}

impl Create for Surface {
    type Config<'a> = &'a Window;
    type CreateError = ExtError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let handle = create_platform_surface(context, config)?;
        let loader: khr::Surface = context.load();
        let PhysicalSize { width, height } = config.inner_size();
        Ok(Self {
            handle,
            loader,
            extent: vk::Extent2D { width, height },
        })
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
    pub extent: vk::Extent2D,
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
    ) -> PhysicalDeviceResult<Self> {
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
            .ok_or(PhysicalDeviceError::MissingSurfaceSupport)?;
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
            Err(PhysicalDeviceError::MissingSurfaceSupport)?;
        }
        let capabilities = unsafe {
            surface
                .loader
                .get_physical_device_surface_capabilities(physical_device, surface.handle)?
        };
        let mut extent = if capabilities.current_extent.width == u32::MAX
            && capabilities.current_extent.height == u32::MAX
        {
            surface.extent
        } else {
            capabilities.current_extent
        };
        extent.width = extent.width.clamp(
            capabilities.min_image_extent.width,
            capabilities.max_image_extent.width,
        );
        extent.height = extent.height.clamp(
            capabilities.min_image_extent.height,
            capabilities.max_image_extent.height,
        );
        Ok(Self {
            extent,
            present_mode,
            surface_format,
            supported_queue_families,
            capabilities,
        })
    }

    pub fn get_current_extent(&self) -> vk::Extent2D {
        self.extent
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
