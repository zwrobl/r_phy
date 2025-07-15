use std::{convert::Infallible, ffi::c_void, fmt::Debug, marker::PhantomData, ops::Deref};

use ash::{prelude::VkResult, vk};
use type_kit::{Create, Destroy, DestroyResult, FromGuard};

use crate::context::{
    device::memory::{AllocReqTyped, MemoryProperties},
    error::{AllocatorResult, ResourceError},
    Context,
};

use super::Resource;

#[derive(Debug, Clone, Copy)]
pub struct MemoryAllocateInfo<M: MemoryProperties> {
    info: vk::MemoryAllocateInfo,
    _phantom: PhantomData<M>,
}

impl Context {
    #[inline]
    pub fn get_memory_allocate_info<M: MemoryProperties>(
        &self,
        req: AllocReqTyped<M>,
    ) -> AllocatorResult<MemoryAllocateInfo<M>> {
        Ok(MemoryAllocateInfo {
            info: vk::MemoryAllocateInfo {
                allocation_size: req.requirements().size,
                memory_type_index: self.get_memory_type_index(req)?,
                ..Default::default()
            },
            _phantom: PhantomData,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRaw {
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,
    _type_index: u32,
    // Extract map functionallity to dedicated helper structure, to be used wrapped in Optional here,
    // Optional<MemoryMapper> to be returned by type function provided by MemoryProperties trait
    ptr: Option<*mut c_void>,
    map_count: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Memory<M: MemoryProperties> {
    memory: MemoryRaw,
    _phantom: PhantomData<M>,
}

impl<M: MemoryProperties> Create for Memory<M> {
    type Config<'a> = MemoryAllocateInfo<M>;
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let MemoryAllocateInfo { info, .. } = config;
        let memory = Memory {
            memory: MemoryRaw {
                memory: unsafe { context.allocate_memory(&info, None)? },
                size: info.allocation_size,
                _type_index: info.memory_type_index,
                ptr: None,
                map_count: 0,
            },
            _phantom: PhantomData,
        };
        Ok(memory)
    }
}

impl<M: MemoryProperties> Memory<M> {
    pub fn map(&mut self, context: &Context) -> VkResult<*mut c_void> {
        if self.memory.map_count == 0 && self.memory.ptr.is_none() {
            let ptr = unsafe {
                context.map_memory(
                    self.memory.memory,
                    0,
                    self.memory.size,
                    vk::MemoryMapFlags::empty(),
                )?
            };
            self.memory.ptr = Some(ptr);
        }
        self.memory.map_count += 1;
        Ok(self.memory.ptr.unwrap())
    }

    pub fn unmap(&mut self, context: &Context) {
        self.memory.map_count -= 1;
        if self.memory.map_count == 0 && self.memory.ptr.is_some() {
            unsafe { context.unmap_memory(self.memory.memory) };
            self.memory.ptr = None;
        }
    }
}

impl<M: MemoryProperties> Deref for Memory<M> {
    type Target = vk::DeviceMemory;

    fn deref(&self) -> &Self::Target {
        &self.memory.memory
    }
}

impl Destroy for MemoryRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.free_memory(self.memory, None);
        }
        Ok(())
    }
}

impl<M: MemoryProperties> Destroy for Memory<M> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.memory.destroy(context)
    }
}

impl<M: MemoryProperties> FromGuard for Memory<M> {
    type Inner = MemoryRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self.memory
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Memory {
            memory: inner,
            _phantom: PhantomData,
        }
    }
}

impl<M: MemoryProperties> Resource for Memory<M> {
    type RawType = MemoryRaw;
}
