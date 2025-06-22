use std::{convert::Infallible, ffi::c_void, fmt::Debug};

use ash::vk;
use type_kit::{Create, Destroy, DestroyResult, FromGuard, TypeGuardUnlocked};

use crate::context::{error::ResourceError, Context};

use super::Resource;

#[derive(Debug, Clone, Copy)]
pub struct MemoryAllocateInfo {
    info: vk::MemoryAllocateInfo,
}

impl Default for MemoryAllocateInfo {
    #[inline]
    fn default() -> Self {
        Self {
            info: vk::MemoryAllocateInfo::default(),
        }
    }
}

impl MemoryAllocateInfo {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_memory_type_index(mut self, memory_type_index: u32) -> Self {
        self.info.memory_type_index = memory_type_index;
        self
    }

    #[inline]
    pub fn with_allocation_size(mut self, allocation_size: vk::DeviceSize) -> Self {
        self.info.allocation_size = allocation_size;
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Memory {
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
    type_index: u32,
    ptr: Option<*mut c_void>,
}

impl Create for Memory {
    type Config<'a> = MemoryAllocateInfo;
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let MemoryAllocateInfo { info, .. } = config;
        let memory = Memory {
            memory: unsafe { context.allocate_memory(&info, None)? },
            size: info.allocation_size,
            type_index: info.memory_type_index,
            ptr: None,
        };
        Ok(memory)
    }
}

impl Destroy for Memory {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.free_memory(self.memory, None);
        }
        Ok(())
    }
}

impl From<TypeGuardUnlocked<Memory, Memory>> for Memory {
    #[inline]
    fn from(guard: TypeGuardUnlocked<Memory, Memory>) -> Self {
        guard.into_inner()
    }
}

impl FromGuard for Memory {
    type Inner = Memory;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self
    }
}

impl Resource for Memory {
    type RawType = Self;
}
