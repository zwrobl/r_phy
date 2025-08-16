use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use graphics::error::GraphicsError;

#[derive(Debug)]
pub enum SystemError {
    EventLoopError(winit::error::EventLoopError),
    ExternalError(winit::error::ExternalError),
    OsError(winit::error::OsError),
    MissingWindowConfiguration,
    GraphicsError(GraphicsError),
}

impl Display for SystemError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemError::EventLoopError(err) => write!(f, "Event loop error: {}", err),
            SystemError::ExternalError(err) => write!(f, "External error: {}", err),
            SystemError::OsError(err) => write!(f, "OS error: {}", err),
            SystemError::MissingWindowConfiguration => write!(f, "Missing window configuration"),
            SystemError::GraphicsError(err) => write!(f, "Graphics error: {}", err),
        }
    }
}

impl Error for SystemError {}

pub type SystemResult<T> = Result<T, SystemError>;

impl From<winit::error::EventLoopError> for SystemError {
    fn from(error: winit::error::EventLoopError) -> Self {
        SystemError::EventLoopError(error)
    }
}

impl From<winit::error::ExternalError> for SystemError {
    fn from(error: winit::error::ExternalError) -> Self {
        SystemError::ExternalError(error)
    }
}

impl From<winit::error::OsError> for SystemError {
    fn from(error: winit::error::OsError) -> Self {
        SystemError::OsError(error)
    }
}

impl From<GraphicsError> for SystemError {
    fn from(error: GraphicsError) -> Self {
        SystemError::GraphicsError(error)
    }
}
