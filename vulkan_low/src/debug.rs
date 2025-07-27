#![allow(unused)]

use std::{
    convert::Infallible,
    error::Error,
    ffi::{c_char, c_void, CStr},
};

use ash::{extensions::ext, vk};
use colored::{self, Colorize};
use type_kit::{Create, Destroy, DestroyResult};

use super::{
    error::{VkError, VkResult},
    Instance,
};

unsafe extern "system" fn debug_messenger_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    message: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    let message_severity = match message_severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => "ERROR".red(),
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => "WARNING".yellow(),
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => "INFO".blue(),
        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => "VERBOSE".dimmed(),
        _ => "UNKNOWN".magenta(),
    }
    .bold();
    let message_type = match message_type {
        vk::DebugUtilsMessageTypeFlagsEXT::GENERAL => "GENERAL".blue(),
        vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE => "PERFORMANCE".yellow(),
        vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION => "VALIDATION".red(),
        vk::DebugUtilsMessageTypeFlagsEXT::DEVICE_ADDRESS_BINDING => {
            "DEVICE_ADDRESS_BINDING".dimmed()
        }
        _ => "UNKNOWN".magenta(),
    }
    .bold();
    let message = CStr::from_ptr((*message).p_message).to_string_lossy();
    println!("[{}][{}]:{}", message_severity, message_type, message);
    vk::FALSE
}

pub(super) struct DebugUtils {
    messenger: vk::DebugUtilsMessengerEXT,
    loader: ext::DebugUtils,
}

impl DebugUtils {
    pub fn create_info() -> vk::DebugUtilsMessengerCreateInfoEXT {
        vk::DebugUtilsMessengerCreateInfoEXT {
            message_severity: vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            message_type: vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                | vk::DebugUtilsMessageTypeFlagsEXT::GENERAL,
            pfn_user_callback: Some(debug_messenger_callback),
            ..Default::default()
        }
    }

    pub fn iterate_required_extensions() -> impl Iterator<Item = &'static CStr> {
        const REQUIRED_EXTENSIONS: [&CStr; 1] = [ext::DebugUtils::name()];
        REQUIRED_EXTENSIONS.into_iter()
    }

    fn iterate_required_layers() -> impl Iterator<Item = &'static CStr> {
        const REQUIRED_LAYERS: [&CStr; 1] =
            [unsafe { &CStr::from_bytes_with_nul_unchecked(b"VK_LAYER_KHRONOS_validation\0") }];
        REQUIRED_LAYERS.into_iter()
    }

    pub fn check_required_layer_support(entry: &ash::Entry) -> VkResult<Vec<*const c_char>> {
        let supported_layers = entry.enumerate_instance_layer_properties()?;
        let supported =
            Self::iterate_required_layers().try_fold(Vec::new(), |mut supported, req| {
                supported_layers
                    .iter()
                    .any(|sup| unsafe { CStr::from_ptr(&sup.layer_name as *const _) } == req)
                    .then(|| {
                        supported.push(req.as_ptr());
                        supported
                    })
                    .ok_or(VkError::LayerNotSupported(req))
            })?;
        Ok(supported)
    }
}

impl Create for DebugUtils {
    type Config<'a> = ();
    type CreateError = VkError;

    fn create<'a>(
        _config: Self::Config<'a>,
        context: Self::Context<'a>,
    ) -> Result<Self, Self::CreateError> {
        let loader: ext::DebugUtils = context.load();
        let messenger = unsafe { loader.create_debug_utils_messenger(&Self::create_info(), None)? };
        Ok(Self { messenger, loader })
    }
}

impl Destroy for DebugUtils {
    type Context<'a> = &'a Instance;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, _: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            self.loader
                .destroy_debug_utils_messenger(self.messenger, None);
        }
        Ok(())
    }
}
