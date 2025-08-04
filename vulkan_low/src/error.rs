use std::{
    error::Error,
    ffi::CStr,
    fmt::{Display, Formatter},
};

use ash::{vk, LoadingError};
use type_kit::{DropGuard, GenCollectionError, TypeGuard, TypeGuardError};
use winit::raw_window_handle::HandleError;

use crate::{
    device::error::DeviceError, memory::error::MemoryError, resources::error::ResourceError,
};

pub type SafeGuardError<T> = (DropGuard<TypeGuard<T>>, TypeGuardError);

#[derive(Debug)]
pub enum ExtError {
    Vulkan(vk::Result),
    Loading(LoadingError),
    Window(HandleError),
    Collection(GenCollectionError),
    TypeGuard(TypeGuardError),
}

impl Display for ExtError {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtError::Vulkan(error) => write!(f, "VkResult: {}", error),
            ExtError::Loading(error) => write!(f, "{}", error),
            ExtError::Window(error) => write!(f, "{}", error),
            ExtError::Collection(error) => write!(f, "{}", error),
            ExtError::TypeGuard(error) => write!(f, "{}", error),
        }
    }
}

impl Error for ExtError {}

impl From<vk::Result> for ExtError {
    #[inline]
    fn from(error: vk::Result) -> Self {
        ExtError::Vulkan(error)
    }
}

impl From<LoadingError> for ExtError {
    #[inline]
    fn from(error: LoadingError) -> Self {
        ExtError::Loading(error)
    }
}

impl From<HandleError> for ExtError {
    #[inline]
    fn from(error: HandleError) -> Self {
        ExtError::Window(error)
    }
}

impl From<TypeGuardError> for ExtError {
    #[inline]
    fn from(error: TypeGuardError) -> Self {
        ExtError::TypeGuard(error)
    }
}

impl From<GenCollectionError> for ExtError {
    #[inline]
    fn from(error: GenCollectionError) -> Self {
        ExtError::Collection(error)
    }
}

pub type ExtResult<T> = Result<T, ExtError>;

#[derive(Debug)]
pub enum InstanceError {
    ExtensionNotSupported(&'static CStr),
    LayerNotSupported(&'static CStr),
    ExtError(ExtError),
}

impl Display for InstanceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceError::ExtensionNotSupported(name) => {
                write!(f, "Extension not supported: {}", name.to_string_lossy())
            }
            InstanceError::LayerNotSupported(name) => {
                write!(f, "Layer not supported: {}", name.to_string_lossy())
            }
            InstanceError::ExtError(error) => write!(f, "{}", error),
        }
    }
}

impl<E: Into<ExtError>> From<E> for InstanceError {
    #[inline]
    fn from(error: E) -> Self {
        InstanceError::ExtError(error.into())
    }
}

impl Error for InstanceError {}

pub type InstanceResult<T> = Result<T, InstanceError>;

#[derive(Debug)]
pub enum VkError {
    MemoryError(MemoryError),
    ResourceError(ResourceError),
    DeviceError(DeviceError),
    InstanceError(InstanceError),
    ExtError(ExtError),
}

impl Display for VkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VkError::MemoryError(error) => write!(f, "Allocator error: {}", error),
            VkError::ResourceError(error) => write!(f, "Resource error: {}", error),
            VkError::DeviceError(error) => write!(f, "Device error: {}", error),
            VkError::ExtError(error) => write!(f, "Ash error: {}", error),
            VkError::InstanceError(error) => write!(f, "Instance error: {}", error),
        }
    }
}

impl Error for VkError {}

impl<E: Into<ExtError>> From<E> for VkError {
    fn from(error: E) -> Self {
        VkError::ExtError(error.into())
    }
}

impl From<InstanceError> for VkError {
    fn from(error: InstanceError) -> Self {
        VkError::InstanceError(error)
    }
}

impl From<ResourceError> for VkError {
    fn from(error: ResourceError) -> Self {
        VkError::ResourceError(error)
    }
}

impl From<MemoryError> for VkError {
    fn from(error: MemoryError) -> Self {
        VkError::MemoryError(error)
    }
}

impl From<DeviceError> for VkError {
    fn from(error: DeviceError) -> Self {
        VkError::DeviceError(error)
    }
}

pub type VkResult<T> = Result<T, VkError>;
