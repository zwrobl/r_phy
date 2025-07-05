use std::{convert::Infallible, ffi::c_void, fmt::Debug, ops::Deref};

use ash::{prelude::VkResult, vk};
use type_kit::{Create, Destroy, DestroyResult, FromGuard, TypeGuardUnlocked};

use crate::context::{
    device::memory::AllocReq,
    error::{AllocatorResult, ResourceError},
    Context,
};

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

impl Context {
    #[inline]
    pub fn get_memory_allocate_info(&self, req: AllocReq) -> AllocatorResult<MemoryAllocateInfo> {
        Ok(MemoryAllocateInfo {
            info: vk::MemoryAllocateInfo {
                allocation_size: req.requirements().size,
                memory_type_index: self.get_memory_type_index(req)?,
                ..Default::default()
            },
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Memory {
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
    _type_index: u32,
    // Extract map functionallity to dedicated helper structure, to be used wrapped in Optional here,
    // Optional<MemoryMapper> to be returned by type function provided by MemoryProperties trait
    ptr: Option<*mut c_void>,
    map_count: u32,
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
            _type_index: info.memory_type_index,
            ptr: None,
            map_count: 0,
        };
        Ok(memory)
    }
}

impl Memory {
    pub fn map(&mut self, context: &Context) -> VkResult<*mut c_void> {
        if self.map_count == 0 && self.ptr.is_none() {
            let ptr = unsafe {
                context.map_memory(self.memory, 0, self.size, vk::MemoryMapFlags::empty())?
            };
            self.ptr = Some(ptr);
        }
        self.map_count += 1;
        Ok(self.ptr.unwrap())
    }

    pub fn unmap(&mut self, context: &Context) {
        self.map_count -= 1;
        if self.map_count == 0 && self.ptr.is_some() {
            unsafe { context.unmap_memory(self.memory) };
            self.ptr = None;
        }
    }
}

impl Deref for Memory {
    type Target = vk::DeviceMemory;

    fn deref(&self) -> &Self::Target {
        &self.memory
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
