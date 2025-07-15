use std::{
    error::Error,
    ffi::CStr,
    fmt::{Display, Formatter},
    io, sync,
};

use ash::vk;
use png::{BitDepth, ColorType};
use type_kit::{GenCollectionError, GuardCollectionError, TypeGuardConversionError};
use winit::raw_window_handle::HandleError;

use super::device::resources::image::ImageCubeFace;

#[derive(Debug, Clone, Copy)]
pub enum AllocatorError {
    InvalidConfiguration,
    UnsupportedMemoryType,
    InvalidAllocationIndex,
    OutOfMemory,
    LegacyAllocError(AllocError),
}

impl Display for AllocatorError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            AllocatorError::LegacyAllocError(error) => write!(f, "{}", error),
            AllocatorError::InvalidConfiguration => write!(f, "Invalid configuration"),
            AllocatorError::UnsupportedMemoryType => write!(f, "Unsupported memory type"),
            AllocatorError::InvalidAllocationIndex => write!(f, "Invalid allocation index"),
            AllocatorError::OutOfMemory => write!(f, "Ount of memory"),
        }
    }
}

impl From<AllocError> for AllocatorError {
    fn from(error: AllocError) -> Self {
        AllocatorError::LegacyAllocError(error)
    }
}

impl Error for AllocatorError {}

pub type AllocatorResult<T> = Result<T, AllocatorError>;

#[derive(Debug, Clone, Copy)]
enum CollectionError {
    GenCollection(GenCollectionError),
    GuardCollection(GuardCollectionError),
}

impl Display for CollectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectionError::GenCollection(error) => write!(f, "{}", error),
            CollectionError::GuardCollection(error) => write!(f, "{}", error),
        }
    }
}

#[derive(Debug)]
pub enum ResourceError {
    AllocatorError(AllocatorError),
    TypeConversion(TypeGuardConversionError),
    CollectionError(CollectionError),
    ImageError(ImageError),
    VkError(vk::Result),
}

impl Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceError::AllocatorError(error) => write!(f, "{}", error),
            ResourceError::TypeConversion(error) => write!(f, "{}", error),
            ResourceError::CollectionError(error) => write!(f, "{}", error),
            ResourceError::ImageError(error) => write!(f, "{}", error),
            ResourceError::VkError(error) => write!(f, "Vulkan error: {:?}", error),
        }
    }
}

impl Error for ResourceError {}

impl From<AllocatorError> for ResourceError {
    fn from(error: AllocatorError) -> Self {
        ResourceError::AllocatorError(error)
    }
}

impl From<TypeGuardConversionError> for ResourceError {
    fn from(error: TypeGuardConversionError) -> Self {
        ResourceError::TypeConversion(error)
    }
}

impl From<GenCollectionError> for ResourceError {
    fn from(error: GenCollectionError) -> Self {
        ResourceError::CollectionError(CollectionError::GenCollection(error))
    }
}

impl From<GuardCollectionError> for ResourceError {
    fn from(error: GuardCollectionError) -> Self {
        ResourceError::CollectionError(CollectionError::GuardCollection(error))
    }
}

impl From<ImageError> for ResourceError {
    fn from(error: ImageError) -> Self {
        ResourceError::ImageError(error)
    }
}

impl From<vk::Result> for ResourceError {
    fn from(error: vk::Result) -> Self {
        ResourceError::VkError(error)
    }
}

pub type ResourceResult<T> = Result<T, ResourceError>;

#[derive(Debug)]
pub enum ShaderError {
    UnknowStage(String),
    InvalidFile(String),
    FileError(io::Error),
    VkError(vk::Result),
}

impl Display for ShaderError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            ShaderError::UnknowStage(stage) => {
                write!(f, "Unknown shader file type extension: {}!", stage)
            }
            ShaderError::InvalidFile(file) => {
                write!(
                    f,
                    "Shader module path is mising file name component: {}",
                    file
                )
            }
            ShaderError::FileError(err) => write!(f, "File error: {}", err),
            ShaderError::VkError(err) => write!(f, "Vulkan error: {}", err),
        }
    }
}

impl Error for ShaderError {}

pub type ShaderResult<T> = Result<T, ShaderError>;

impl From<io::Error> for ShaderError {
    fn from(err: io::Error) -> Self {
        ShaderError::FileError(err)
    }
}

impl From<vk::Result> for ShaderError {
    fn from(err: vk::Result) -> Self {
        ShaderError::VkError(err)
    }
}

#[derive(Debug)]
pub enum ImageError {
    FileError(io::Error),
    PngDecoderError(png::DecodingError),
    UnsupportedFormat(ColorType, BitDepth),
    InvalidCubeMap(String),
    MissingCubeMapData(ImageCubeFace),
    ExhaustedImageRead,
}

impl Display for ImageError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            ImageError::ExhaustedImageRead => write!(f, "Exhausted image read"),
            ImageError::MissingCubeMapData(entry) => {
                write!(f, "Missing cubemap entry: {:?}", entry)
            }
            ImageError::InvalidCubeMap(entry) => {
                write!(f, "Invalid cubemap directory entry: {}", entry)
            }
            ImageError::FileError(err) => write!(f, "File error: {}", err),
            ImageError::PngDecoderError(err) => write!(f, "PNG decoder error: {}", err),
            ImageError::UnsupportedFormat(color_type, bit_depth) => {
                write!(
                    f,
                    "Unsupported png Image ColorType: {:?} and BitDepth: {:?}!",
                    color_type, bit_depth
                )
            }
        }
    }
}

impl Error for ImageError {}

impl From<io::Error> for ImageError {
    fn from(err: io::Error) -> Self {
        ImageError::FileError(err)
    }
}

impl From<png::DecodingError> for ImageError {
    fn from(err: png::DecodingError) -> Self {
        ImageError::PngDecoderError(err)
    }
}

pub type ImageResult<T> = Result<T, ImageError>;

#[derive(Debug, Clone, Copy)]
pub enum AllocError {
    OutOfMemory,
    UnsupportedMemoryType,
    VulkanError(vk::Result),
}

impl Display for AllocError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            AllocError::OutOfMemory => write!(f, "Out of memory"),
            AllocError::UnsupportedMemoryType => write!(f, "Unsupported memory type"),
            AllocError::VulkanError(err) => write!(f, "Vulkan error: {}", err),
        }
    }
}

impl From<vk::Result> for AllocError {
    fn from(err: vk::Result) -> Self {
        AllocError::VulkanError(err)
    }
}

impl Error for AllocError {}

pub type AllocResult<T> = Result<T, AllocError>;

#[derive(Debug)]
pub enum DeviceNotSuitable {
    InvalidDeviceType,
    MissingSurfaceSupport,
    MissingDepthAndStencilFormat,
    MissingQueueFamilyIndex(&'static str),
    ExtensionNotSupported(&'static CStr),
    VkError(vk::Result),
}

impl Display for DeviceNotSuitable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Device not suitable")
    }
}

impl Error for DeviceNotSuitable {}

impl From<vk::Result> for DeviceNotSuitable {
    fn from(error: vk::Result) -> Self {
        DeviceNotSuitable::VkError(error)
    }
}

#[derive(Debug)]
pub enum VkError {
    AllocatorError(AllocatorError),
    ResourceError(ResourceError),
    ShaderError(ShaderError),
    ImageError(ImageError),
    AllocationError(AllocError),
    NoSuitablePhysicalDevice(Vec<DeviceNotSuitable>),
    ExtensionNotSupported(&'static CStr),
    LayerNotSupported(&'static CStr),
    VkError(vk::Result),
    LoadError(ash::LoadingError),
    WindowError(HandleError),
    // Temporary LockError handling, storing the PoisonError.to_string() to elide the lock Guard type
    LockError(String),
}

impl Display for VkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VkError::AllocatorError(error) => write!(f, "Allocator error: {}", error),
            VkError::ResourceError(error) => write!(f, "Resource error: {}", error),
            VkError::ShaderError(error) => write!(f, "Shader error: {}", error),
            VkError::LockError(error) => write!(f, "Lock error: {}", error),
            VkError::ImageError(error) => write!(f, "Image error: {}", error),
            VkError::AllocationError(error) => write!(f, "Allocation error: {}", error),
            VkError::NoSuitablePhysicalDevice(devices) => {
                write!(f, "No suitable device found: {:?}", devices)
            }
            VkError::ExtensionNotSupported(extension) => {
                write!(
                    f,
                    "Extension not supported: {}",
                    extension.to_string_lossy()
                )
            }
            VkError::LayerNotSupported(layer) => {
                write!(f, "Layer not supported: {}", layer.to_string_lossy())
            }
            VkError::VkError(error) => write!(f, "Vulkan error: {:?}", error),
            VkError::LoadError(error) => write!(f, "Loading error: {:?}", error),
            VkError::WindowError(error) => write!(f, "Window error: {:?}", error),
        }
    }
}

impl Error for VkError {}

impl From<vk::Result> for VkError {
    fn from(error: vk::Result) -> Self {
        VkError::VkError(error)
    }
}

impl From<ash::LoadingError> for VkError {
    fn from(error: ash::LoadingError) -> Self {
        VkError::LoadError(error)
    }
}

impl From<HandleError> for VkError {
    fn from(error: HandleError) -> Self {
        VkError::WindowError(error)
    }
}

impl From<AllocError> for VkError {
    fn from(error: AllocError) -> Self {
        VkError::AllocationError(error)
    }
}

impl From<ImageError> for VkError {
    fn from(error: ImageError) -> Self {
        VkError::ImageError(error)
    }
}

impl<T> From<sync::PoisonError<T>> for VkError {
    fn from(error: sync::PoisonError<T>) -> Self {
        VkError::LockError(error.to_string())
    }
}

impl From<ShaderError> for VkError {
    fn from(error: ShaderError) -> Self {
        VkError::ShaderError(error)
    }
}

impl From<ResourceError> for VkError {
    fn from(error: ResourceError) -> Self {
        VkError::ResourceError(error)
    }
}

impl From<AllocatorError> for VkError {
    fn from(error: AllocatorError) -> Self {
        VkError::AllocatorError(error)
    }
}

pub type VkResult<T> = Result<T, VkError>;
