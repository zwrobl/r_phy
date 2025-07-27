pub mod frame;
pub mod memory;
pub mod raw;
pub mod renderer;
pub mod resources;

use super::{
    error::{DeviceNotSuitable, VkError, VkResult},
    Instance,
};

use super::surface::{PhysicalDeviceSurfaceProperties, Surface};
use ash::{self, vk};
use colored::Colorize;
use std::convert::Infallible;
use std::ffi::c_char;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    ffi::CStr,
};
use type_kit::{Create, Destroy, DestroyResult};

#[derive(Debug, Clone, Copy)]
struct QueueFamilies {
    graphics: u32,
    compute: u32,
    transfer: u32,
}

impl QueueFamilies {
    pub fn get(
        properties: &PhysicalDeviceProperties,
        surface_properties: &PhysicalDeviceSurfaceProperties,
    ) -> Result<Self, DeviceNotSuitable> {
        let mut queue_usages = HashMap::new();
        let mut try_use_queue_family = |queue: &mut Option<u32>, queue_family_index: u32| {
            if match queue {
                None => true,
                Some(current_index) if queue_usages[current_index] > 1 => {
                    queue_usages.entry(*current_index).and_modify(|n| *n -= 1);
                    true
                }
                _ => false,
            } {
                queue.replace(queue_family_index);
                queue_usages
                    .entry(queue_family_index)
                    .and_modify(|n| *n += 1)
                    .or_insert(1);
            }
        };
        let (mut graphics, mut compute, mut transfer) = (None, None, None);
        for &(properties, queue_family_index) in &properties.queue_families {
            if graphics.is_none()
                && properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                && surface_properties
                    .supported_queue_families
                    .contains(&queue_family_index)
            {
                try_use_queue_family(&mut graphics, queue_family_index);
            }
            if properties.queue_flags.contains(vk::QueueFlags::COMPUTE) {
                try_use_queue_family(&mut compute, queue_family_index);
            }
            if transfer.is_none() && properties.queue_flags.contains(vk::QueueFlags::TRANSFER) {
                try_use_queue_family(&mut transfer, queue_family_index);
            }
        }
        Ok(Self {
            graphics: graphics.ok_or(DeviceNotSuitable::MissingQueueFamilyIndex(&"Graphics"))?,
            compute: compute.ok_or(DeviceNotSuitable::MissingQueueFamilyIndex(&"Compute"))?,
            transfer: transfer.ok_or(DeviceNotSuitable::MissingQueueFamilyIndex(&"Transfer"))?,
        })
    }
}

struct DeviceQueueBuilder {
    queue_families: QueueFamilies,
    unique: HashSet<u32>,
}

impl DeviceQueueBuilder {
    pub fn new(queue_families: QueueFamilies) -> Self {
        let unique = HashSet::<u32>::from_iter([
            queue_families.compute,
            queue_families.graphics,
            queue_families.transfer,
        ]);
        Self {
            queue_families,
            unique,
        }
    }

    pub fn get_device_queue_create_infos(&self) -> Vec<vk::DeviceQueueCreateInfo> {
        self.unique
            .iter()
            .map(|&queue_family_index| vk::DeviceQueueCreateInfo {
                queue_family_index,
                queue_count: 1,
                p_queue_priorities: &1.0f32,
                ..Default::default()
            })
            .collect()
    }

    pub fn get_device_queues(&self, device: &ash::Device) -> DeviceQueues {
        let quque_map =
            HashMap::<u32, vk::Queue>::from_iter(self.unique.iter().map(|&queue_family_index| {
                (queue_family_index, unsafe {
                    device.get_device_queue(queue_family_index, 0)
                })
            }));
        DeviceQueues {
            graphics: quque_map[&self.queue_families.graphics],
            compute: quque_map[&self.queue_families.compute],
            transfer: quque_map[&self.queue_families.transfer],
        }
    }
}

#[derive(Debug, Clone)]
pub struct PhysicalDeviceProperties {
    enabled_features: vk::PhysicalDeviceFeatures,
    generic: vk::PhysicalDeviceProperties,
    memory: vk::PhysicalDeviceMemoryProperties,
    enabled_extension_names: Vec<*const c_char>,
    queue_families: Vec<(vk::QueueFamilyProperties, u32)>,
}

impl PhysicalDeviceProperties {
    pub fn get_enabled_features(
        features: &vk::PhysicalDeviceFeatures,
    ) -> vk::PhysicalDeviceFeatures {
        vk::PhysicalDeviceFeatures {
            sample_rate_shading: features.sample_rate_shading,
            ..Default::default()
        }
    }

    pub fn get(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self, DeviceNotSuitable> {
        let generic = unsafe { instance.get_physical_device_properties(physical_device) };
        let features = unsafe { instance.get_physical_device_features(physical_device) };
        let memory = unsafe { instance.get_physical_device_memory_properties(physical_device) };
        if generic.device_type != vk::PhysicalDeviceType::DISCRETE_GPU
            && generic.device_type != vk::PhysicalDeviceType::INTEGRATED_GPU
        {
            Err(DeviceNotSuitable::InvalidDeviceType)?;
        }
        let enabled_features = Self::get_enabled_features(&features);
        let enabled_extension_names =
            Self::check_required_device_extension_support(instance, physical_device)?;
        let queue_families = Self::get_device_queue_families_properties(instance, physical_device);
        Ok(Self {
            enabled_features,
            memory,
            generic,
            enabled_extension_names,
            queue_families,
        })
    }

    fn check_required_device_extension_support(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Vec<*const c_char>, DeviceNotSuitable> {
        let supported_extensions =
            unsafe { instance.enumerate_device_extension_properties(physical_device)? };
        let required_extensions = raw::resources::swapchain::required_extensions();
        let enabled_extension_names =
            required_extensions
                .iter()
                .try_fold(Vec::new(), |mut supported, req| {
                    supported_extensions
                    .iter()
                    .any(|sup| unsafe { CStr::from_ptr(&sup.extension_name as *const _) } == *req)
                    .then(|| {
                        supported.push(req.as_ptr());
                        supported
                    })
                    .ok_or(DeviceNotSuitable::ExtensionNotSupported(req))
                })?;
        Ok(enabled_extension_names)
    }

    fn get_device_queue_families_properties(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
    ) -> Vec<(vk::QueueFamilyProperties, u32)> {
        let mut quque_properties =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) }
                .into_iter()
                .zip(0_u32..)
                .collect::<Vec<_>>();
        quque_properties.sort_by_key(|(properties, _)| {
            [
                vk::QueueFlags::GRAPHICS,
                vk::QueueFlags::COMPUTE,
                vk::QueueFlags::TRANSFER,
            ]
            .iter()
            .fold(0, |n, &flag| {
                if properties.queue_flags.contains(flag) {
                    n + 1
                } else {
                    n
                }
            })
        });
        quque_properties
    }
}

#[derive(Debug, Clone, Copy)]
struct AttachmentFormats {
    color: vk::Format,
    depth_stencil: vk::Format,
}

#[derive(Debug, Clone, Copy)]
pub struct AttachmentProperties {
    formats: AttachmentFormats,
    msaa_samples: vk::SampleCountFlags,
}

impl AttachmentProperties {
    const PREFERRED_DEPTH_FORMATS: &'static [vk::Format] = &[
        vk::Format::D32_SFLOAT_S8_UINT,
        vk::Format::D24_UNORM_S8_UINT,
        vk::Format::D16_UNORM_S8_UINT,
    ];

    pub fn get(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        properties: &PhysicalDeviceProperties,
        surface_properties: &PhysicalDeviceSurfaceProperties,
    ) -> Result<Self, DeviceNotSuitable> {
        let color = surface_properties.surface_format.format;
        let depth_stencil = *Self::PREFERRED_DEPTH_FORMATS
            .iter()
            .find(|&&pref| {
                let format_properties = unsafe {
                    instance.get_physical_device_format_properties(physical_device, pref)
                };
                format_properties
                    .optimal_tiling_features
                    .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
            })
            .ok_or(DeviceNotSuitable::MissingDepthAndStencilFormat)?;
        let msaa_samples = [
            vk::SampleCountFlags::TYPE_64,
            vk::SampleCountFlags::TYPE_32,
            vk::SampleCountFlags::TYPE_16,
            vk::SampleCountFlags::TYPE_8,
            vk::SampleCountFlags::TYPE_4,
            vk::SampleCountFlags::TYPE_2,
        ]
        .into_iter()
        .find(|&sample_count| {
            (properties.generic.limits.framebuffer_color_sample_counts
                & properties.generic.limits.framebuffer_depth_sample_counts)
                .contains(sample_count)
        })
        .unwrap_or(vk::SampleCountFlags::TYPE_1);

        Ok(Self {
            formats: AttachmentFormats {
                color,
                depth_stencil,
            },
            msaa_samples,
        })
    }
}

#[derive(Debug)]
struct PhysicalDevice {
    properties: PhysicalDeviceProperties,
    surface_properties: PhysicalDeviceSurfaceProperties,
    attachment_properties: AttachmentProperties,
    queue_families: QueueFamilies,
    handle: vk::PhysicalDevice,
}

impl PhysicalDevice {
    fn get_physical_device_name(&self) -> &CStr {
        unsafe { CStr::from_ptr(&self.properties.generic.device_name as *const _) }
    }
}

#[derive(Debug)]
struct DeviceQueues {
    graphics: vk::Queue,
    compute: vk::Queue,
    transfer: vk::Queue,
}

pub struct Device {
    physical_device: PhysicalDevice,
    device_queues: DeviceQueues,
    device: ash::Device,
}

impl Debug for Device {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let device_name = self
            .physical_device
            .get_physical_device_name()
            .to_string_lossy()
            .bold()
            .green();
        f.debug_struct("Device")
            .field("physical_device", &self.physical_device)
            .field("device_queues", &self.device_queues)
            .field("device", &device_name)
            .finish()
    }
}

impl Deref for Device {
    type Target = ash::Device;
    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

fn check_physical_device_suitable(
    physical_device: vk::PhysicalDevice,
    instance: &ash::Instance,
    surface: &Surface,
) -> Result<PhysicalDevice, DeviceNotSuitable> {
    let properties = PhysicalDeviceProperties::get(instance, physical_device)?;
    let surface_properties =
        PhysicalDeviceSurfaceProperties::get(surface, physical_device, &properties.queue_families)?;
    let attachment_properties =
        AttachmentProperties::get(instance, physical_device, &properties, &surface_properties)?;
    let queue_families = QueueFamilies::get(&properties, &surface_properties)?;
    Ok(PhysicalDevice {
        properties,
        surface_properties,
        attachment_properties,
        queue_families,
        handle: physical_device,
    })
}

fn pick_physical_device(instance: &ash::Instance, surface: &Surface) -> VkResult<PhysicalDevice> {
    let (suitable_devices, discarded_devices) = unsafe { instance.enumerate_physical_devices()? }
        .into_iter()
        .map(|physical_device| check_physical_device_suitable(physical_device, instance, surface))
        .partition::<Vec<_>, _>(Result::is_ok);
    let physical_device = suitable_devices
        .into_iter()
        .next()
        .ok_or_else(|| {
            let discarded_devices = discarded_devices
                .into_iter()
                .map(|result| match result {
                    Err(cause) => cause,
                    Ok(..) => unreachable!(),
                })
                .collect();
            VkError::NoSuitablePhysicalDevice(discarded_devices)
        })?
        .unwrap();
    println!(
        "Using {} Physical Device",
        physical_device
            .get_physical_device_name()
            .to_string_lossy()
            .bold()
            .green()
    );
    Ok(physical_device)
}

impl Device {
    pub fn wait_idle(&self) -> Result<(), Box<dyn Error>> {
        unsafe {
            self.device.device_wait_idle()?;
        }
        Ok(())
    }
}

impl Create for Device {
    type Config<'a> = &'a Surface;
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let physical_device = pick_physical_device(context, config)?;
        let queue_builder = DeviceQueueBuilder::new(physical_device.queue_families);
        let device = unsafe {
            context.create_device(
                physical_device.handle,
                &vk::DeviceCreateInfo::builder()
                    .queue_create_infos(&queue_builder.get_device_queue_create_infos())
                    .enabled_extension_names(&physical_device.properties.enabled_extension_names)
                    .enabled_features(&physical_device.properties.enabled_features),
                None,
            )?
        };
        let device_queues = queue_builder.get_device_queues(&device);
        Ok(Self {
            physical_device,
            device_queues,
            device,
        })
    }
}

impl Destroy for Device {
    type Context<'a> = &'a Instance;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            self.device.destroy_device(None);
        }
        Ok(())
    }
}
