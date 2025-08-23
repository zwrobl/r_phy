use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use graphics::error::GraphicsError;
use type_kit::{DropGuard, GuardError, TypeGuard};

use crate::{
    error::{ExtError, SafeGuardError},
    memory::{
        AllocReq, MemoryRaw,
        allocator::{AllocationRaw, MemoryIndexRaw},
    },
};

#[derive(Debug)]
pub enum MemoryError {
    InvalidMemoryIndex {
        index: TypeGuard<MemoryIndexRaw>,
    },
    AllocationGuard {
        error: GuardError<AllocationRaw>,
    },
    MemoryGuard {
        error: SafeGuardError<MemoryRaw>,
    },
    UnexpectedAllocation {
        actual: AllocReq,
        expected: Option<AllocReq>,
    },
    InvalidConfiguration,
    UnsupportedMemoryType,
    OutOfMemory,
    ExtError(ExtError),
}

impl Display for MemoryError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            MemoryError::InvalidMemoryIndex { index } => {
                write!(f, "Invalid memory index: {:?}", index)
            }
            MemoryError::AllocationGuard { error: index } => {
                write!(f, "Allocation type guard error: {:?}", index)
            }
            MemoryError::MemoryGuard { error } => {
                write!(f, "Memory type guard error: {:?}", error)
            }
            MemoryError::UnexpectedAllocation { actual, expected } => {
                write!(
                    f,
                    "Unexpected allocation: {:?}, expected: {:?}",
                    actual, expected
                )
            }
            MemoryError::InvalidConfiguration => write!(f, "Invalid configuration"),
            MemoryError::UnsupportedMemoryType => write!(f, "Unsupported memory type"),
            MemoryError::OutOfMemory => write!(f, "Out of memory"),
            MemoryError::ExtError(error) => write!(f, "{}", error),
        }
    }
}

impl Error for MemoryError {}

impl From<GuardError<MemoryRaw>> for MemoryError {
    #[inline]
    fn from((resource, err): GuardError<MemoryRaw>) -> Self {
        MemoryError::MemoryGuard {
            error: (DropGuard::new(resource), err),
        }
    }
}

impl From<GuardError<AllocationRaw>> for MemoryError {
    #[inline]
    fn from(error: GuardError<AllocationRaw>) -> Self {
        MemoryError::AllocationGuard { error }
    }
}

impl<E: Into<ExtError>> From<E> for MemoryError {
    #[inline]
    fn from(error: E) -> Self {
        MemoryError::ExtError(error.into())
    }
}

pub type MemoryResult<T> = Result<T, MemoryError>;

impl From<MemoryError> for GraphicsError {
    fn from(error: MemoryError) -> Self {
        GraphicsError::External(error.to_string())
    }
}
