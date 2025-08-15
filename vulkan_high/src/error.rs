use std::{
    error::Error,
    fmt::{Display, Formatter},
};

#[derive(Debug)]
pub enum VkError {
    VkError(vulkan_low::error::VkError),
}

impl Error for VkError {}

impl Display for VkError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VkError::VkError(error) => write!(f, "Vulkan error: {}", error),
        }
    }
}

pub type VkResult<T> = Result<T, VkError>;

impl<T: Into<vulkan_low::error::VkError>> From<T> for VkError {
    fn from(error: T) -> Self {
        VkError::VkError(error.into())
    }
}
