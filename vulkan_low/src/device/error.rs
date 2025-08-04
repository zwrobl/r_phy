use std::{
    error::Error,
    ffi::CStr,
    fmt::{Display, Formatter},
};

use crate::error::ExtError;

#[derive(Debug)]
pub enum PhysicalDeviceError {
    InvalidDeviceType,
    MissingSurfaceSupport,
    MissingDepthAndStencilFormat,
    MissingQueueFamilyIndex(&'static str),
    ExtensionNotSupported(&'static CStr),
    ExtError(ExtError),
}

impl Display for PhysicalDeviceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PhysicalDeviceError::InvalidDeviceType => write!(f, "Invalid device type"),
            PhysicalDeviceError::MissingSurfaceSupport => write!(f, "Missing surface support"),
            PhysicalDeviceError::MissingDepthAndStencilFormat => {
                write!(f, "Missing depth and stencil format")
            }
            PhysicalDeviceError::MissingQueueFamilyIndex(name) => {
                write!(f, "Missing queue family index: {}", name)
            }
            PhysicalDeviceError::ExtensionNotSupported(name) => {
                write!(f, "Extension not supported: {}", name.to_string_lossy())
            }
            PhysicalDeviceError::ExtError(error) => write!(f, "{}", error),
        }
    }
}

impl Error for PhysicalDeviceError {}

impl<E: Into<ExtError>> From<E> for PhysicalDeviceError {
    fn from(error: E) -> Self {
        PhysicalDeviceError::ExtError(error.into())
    }
}

pub type PhysicalDeviceResult<T> = Result<T, PhysicalDeviceError>;

#[derive(Debug)]
pub enum DeviceError {
    NoSuitablePhysicalDevice(Vec<PhysicalDeviceError>),
    ExtError(ExtError),
}

impl Display for DeviceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceError::NoSuitablePhysicalDevice(devices) => {
                write!(f, "No suitable physical device found: {:?}", devices)
            }
            DeviceError::ExtError(error) => write!(f, "Ext error: {}", error),
        }
    }
}

impl Error for DeviceError {}

impl<E: Into<ExtError>> From<E> for DeviceError {
    #[inline]
    fn from(error: E) -> Self {
        DeviceError::ExtError(error.into())
    }
}

pub type DeviceResult<T> = Result<T, DeviceError>;
