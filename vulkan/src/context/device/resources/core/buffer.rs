mod persistent;
mod range;
mod staging;
mod uniform;

pub use persistent::*;
pub use range::*;
pub use staging::*;
use type_kit::{Create, Destroy, DestroyResult};
pub use uniform::*;

use ash::vk;

use std::{convert::Infallible, marker::PhantomData, usize};

use crate::context::{
    device::{
        memory::{AllocReq, AllocReqTyped, BindResource, MemoryProperties},
        raw::allocator::{AllocationEntry, AllocatorIndex},
    },
    error::{VkError, VkResult},
    Context,
};

use super::PartialBuilder;

#[derive(Debug, Clone, Copy)]
pub struct BufferInfo<'a> {
    pub size: usize,
    pub usage: vk::BufferUsageFlags,
    pub sharing_mode: vk::SharingMode,
    pub queue_families: &'a [u32],
}

pub struct BufferBuilder<'a, M: MemoryProperties> {
    pub info: BufferInfo<'a>,
    _phantom: PhantomData<M>,
}

impl<'a, M: MemoryProperties> BufferBuilder<'a, M> {
    pub fn new(info: BufferInfo<'a>) -> Self {
        Self {
            info,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct Buffer<M: MemoryProperties> {
    size: usize,
    buffer: vk::Buffer,
    allocation: AllocationEntry<M>,
}

impl<M: MemoryProperties> Buffer<M> {
    pub fn handle(&self) -> vk::Buffer {
        self.buffer
    }

    pub fn size(&self) -> usize {
        self.size
    }
}

#[derive(Debug)]
pub struct BufferPartial<M: MemoryProperties> {
    size: usize,
    req: AllocReqTyped<M>,
    buffer: vk::Buffer,
}

impl<'a, M: MemoryProperties> PartialBuilder<'a> for BufferPartial<M> {
    type Config = BufferBuilder<'a, M>;
    type Target = Buffer<M>;

    fn prepare(config: Self::Config, context: &Context) -> VkResult<Self> {
        let BufferBuilder {
            info:
                BufferInfo {
                    size,
                    usage,
                    sharing_mode,
                    queue_families,
                },
            ..
        } = config;
        let create_info = vk::BufferCreateInfo {
            usage,
            sharing_mode,
            size: size as u64,
            queue_family_index_count: queue_families.len() as u32,
            p_queue_family_indices: queue_families.as_ptr(),
            ..Default::default()
        };
        let buffer = unsafe { context.create_buffer(&create_info, None)? };
        let req = BindResource::new(buffer).get_alloc_req(context);
        Ok(BufferPartial { size, req, buffer })
    }

    fn requirements(&self) -> impl Iterator<Item = AllocReq> {
        [self.req.into()].into_iter()
    }
}

impl<M: MemoryProperties> Create for Buffer<M> {
    type Config<'a> = (BufferPartial<M>, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (
            BufferPartial {
                size, buffer, req, ..
            },
            allocator,
        ) = config;
        let allocation = context.allocate(allocator, req)?;
        context.bind_memory(buffer, allocation)?;
        Ok(Buffer {
            size,
            buffer,
            allocation,
        })
    }
}

impl<M: MemoryProperties> Destroy for Buffer<M> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_buffer(self.buffer, None);
        }
        let _ = context.free(self.allocation);
        Ok(())
    }
}
