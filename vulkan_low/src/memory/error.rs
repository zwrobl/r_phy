use std::{
    error::Error,
    fmt::{Display, Formatter},
};

use type_kit::{BorrowGuardError, GuardError, GuardVec, TypeGuard};

use crate::{
    error::ExtError,
    memory::{
        allocator::{AllocationRaw, MemoryIndexRaw},
        MemoryRaw,
    },
};

#[derive(Debug)]
pub enum MemoryError {
    InvalidMemoryIndex {
        index: TypeGuard<MemoryIndexRaw>,
    },
    AllocationTypeGuard {
        error: GuardError<AllocationRaw>,
    },
    MemoryTypeGuard {
        error: GuardError<MemoryRaw>,
    },
    MemoryBorrow {
        error: BorrowGuardError<MemoryRaw, GuardVec<MemoryRaw>>,
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
            MemoryError::AllocationTypeGuard { error: index } => {
                write!(f, "Allocation type guard error: {:?}", index)
            }
            MemoryError::MemoryTypeGuard { error } => {
                write!(f, "Memory type guard error: {:?}", error)
            }
            MemoryError::MemoryBorrow { error } => write!(f, "Memory borrow error: {:?}", error),
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
    fn from(error: GuardError<MemoryRaw>) -> Self {
        MemoryError::MemoryTypeGuard { error }
    }
}

impl From<GuardError<AllocationRaw>> for MemoryError {
    #[inline]
    fn from(error: GuardError<AllocationRaw>) -> Self {
        MemoryError::AllocationTypeGuard { error }
    }
}

impl From<BorrowGuardError<MemoryRaw, GuardVec<MemoryRaw>>> for MemoryError {
    #[inline]
    fn from(error: BorrowGuardError<MemoryRaw, GuardVec<MemoryRaw>>) -> Self {
        MemoryError::MemoryBorrow { error }
    }
}

impl<E: Into<ExtError>> From<E> for MemoryError {
    #[inline]
    fn from(error: E) -> Self {
        MemoryError::ExtError(error.into())
    }
}

pub type MemoryResult<T> = Result<T, MemoryError>;
