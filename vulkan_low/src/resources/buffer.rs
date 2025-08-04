mod persistent;
mod staging;
mod uniform;

pub use persistent::*;
pub use staging::*;
pub use uniform::*;

use std::{convert::Infallible, ffi::c_void, marker::PhantomData};

use ash::vk;
use type_kit::{Create, CreateResult, Destroy, DestroyResult, DropGuard, FromGuard, TypeGuardVec};

use crate::{
    error::{ResourceError, ResourceResult},
    memory::{
        allocator::{AllocationEntry, AllocationEntryTyped, AllocatorBuilder, AllocatorIndex},
        AllocReqTyped, BindResource, HostCoherent, MemoryProperties,
    },
    resources::Partial,
    Context,
};

use super::Resource;

#[derive(Debug, Clone, Copy)]
pub struct BufferInfo<'a, M: MemoryProperties> {
    create_info: vk::BufferCreateInfo,
    _queue_families: &'a [u32],
    _phantom: PhantomData<M>,
}

#[derive(Debug, Clone, Copy)]
pub enum SharingMode {
    Exclusive,
    Concurrent,
}

impl SharingMode {
    fn get_vk_sharing_mode(self) -> vk::SharingMode {
        match self {
            Self::Exclusive => vk::SharingMode::EXCLUSIVE,
            Self::Concurrent => vk::SharingMode::CONCURRENT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BufferUsage {
    VertexBuffer,
    IndexBuffer,
    TransferDst,
    TransferSrc,
    UniformBuffer,
}

impl BufferUsage {
    fn get_vk_usage_flags(self) -> vk::BufferUsageFlags {
        match self {
            Self::VertexBuffer => vk::BufferUsageFlags::VERTEX_BUFFER,
            Self::IndexBuffer => vk::BufferUsageFlags::INDEX_BUFFER,
            Self::TransferDst => vk::BufferUsageFlags::TRANSFER_DST,
            Self::TransferSrc => vk::BufferUsageFlags::TRANSFER_SRC,
            Self::UniformBuffer => vk::BufferUsageFlags::UNIFORM_BUFFER,
        }
    }
}

#[derive(Debug)]
pub struct BufferInfoBuilder<'a, M: MemoryProperties> {
    size: Option<vk::DeviceSize>,
    usage: Option<vk::BufferUsageFlags>,
    sharing_mode: Option<vk::SharingMode>,
    queue_families: Option<&'a [u32]>,
    _phantom: PhantomData<M>,
}

impl<'a, M: MemoryProperties> BufferInfoBuilder<'a, M> {
    #[inline]
    pub fn new<T: MemoryProperties>() -> BufferInfoBuilder<'static, T> {
        BufferInfoBuilder {
            size: None,
            usage: None,
            sharing_mode: None,
            queue_families: None,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn with_size(self, size: usize) -> Self {
        Self {
            size: Some(size as vk::DeviceSize),
            ..self
        }
    }

    #[inline]
    pub fn with_usage(self, usage: BufferUsage) -> Self {
        let current = self.usage.unwrap_or(vk::BufferUsageFlags::empty());
        Self {
            usage: Some(current | usage.get_vk_usage_flags()),
            ..self
        }
    }

    #[inline]
    pub fn with_sharing_mode(self, sharing_mode: SharingMode) -> Self {
        Self {
            sharing_mode: Some(sharing_mode.get_vk_sharing_mode()),
            ..self
        }
    }

    #[inline]
    pub fn with_queue_families<'b>(self, queue_families: &'b [u32]) -> BufferInfoBuilder<'b, M> {
        BufferInfoBuilder {
            size: self.size,
            usage: self.usage,
            sharing_mode: self.sharing_mode,
            queue_families: Some(queue_families),
            _phantom: PhantomData,
        }
    }

    fn build(self) -> BufferInfo<'a, M> {
        let queue_families = self.queue_families.unwrap();
        let create_info = vk::BufferCreateInfo {
            size: self.size.unwrap(),
            usage: self.usage.unwrap(),
            sharing_mode: self.sharing_mode.unwrap(),
            queue_family_index_count: queue_families.len() as u32,
            p_queue_family_indices: queue_families.as_ptr(),
            ..Default::default()
        };
        BufferInfo {
            create_info,
            _queue_families: queue_families,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct BufferPartial<M: MemoryProperties> {
    buffer: vk::Buffer,
    alloc_req: AllocReqTyped<M>,
}

impl<M: MemoryProperties> Create for BufferPartial<M> {
    type Config<'a> = BufferInfoBuilder<'a, M>;

    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let info = config.build();
        let buffer = unsafe { context.create_buffer(&info.create_info, None)? };
        let alloc_req = BindResource::new(buffer).get_alloc_req(context);
        Ok(BufferPartial { buffer, alloc_req })
    }
}

impl<M: MemoryProperties> Destroy for BufferPartial<M> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_buffer(self.buffer, None);
        }
        Ok(())
    }
}

impl<M: MemoryProperties> Partial for BufferPartial<M> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        builder.with_allocation(self.alloc_req);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BufferRaw {
    buffer: vk::Buffer,
    size: usize,
    ptr: Option<*mut c_void>,
    allocation: AllocationEntry,
}

#[derive(Debug)]
pub struct Buffer<M: MemoryProperties> {
    buffer: vk::Buffer,
    size: usize,
    ptr: Option<*mut c_void>,
    allocation: AllocationEntryTyped<M>,
}

impl<M: MemoryProperties> Buffer<M> {
    #[inline]
    pub fn get_size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn get_ptr(&self) -> Option<*mut c_void> {
        self.ptr
    }

    #[inline]
    pub fn get_vk_buffer(&self) -> vk::Buffer {
        self.buffer
    }
}

impl<M: MemoryProperties> FromGuard for Buffer<M> {
    type Inner = BufferRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        BufferRaw {
            buffer: self.buffer,
            size: self.size,
            ptr: self.ptr,
            allocation: self.allocation.into_inner(),
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            buffer: inner.buffer,
            size: inner.size,
            ptr: inner.ptr,
            allocation: AllocationEntryTyped::<M>::from_inner(inner.allocation),
        }
    }
}

impl<M: MemoryProperties> Resource for Buffer<M> {
    type RawType = BufferRaw;
    type RawCollection = TypeGuardVec<Self::RawType>;
}

impl<M: MemoryProperties> Create for Buffer<M> {
    type Config<'a> = (DropGuard<BufferPartial<M>>, Option<AllocatorIndex>);
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (buffer_partial, allocator) = config;
        let BufferPartial { buffer, alloc_req } = unsafe { buffer_partial.unwrap() };
        let allocation = context.allocate(alloc_req, allocator)?;
        context.bind_memory(buffer, allocation)?;
        let buffer = Buffer {
            buffer,
            size: alloc_req.requirements().size as usize,
            allocation,
            ptr: None,
        };
        Ok(buffer)
    }
}

impl Destroy for BufferRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_buffer(self.buffer, None);
        }
        let _ = context.free_allocation_raw(self.allocation);
        Ok(())
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

impl Buffer<HostCoherent> {
    #[inline]
    pub fn map_memory(&mut self, context: &Context) -> ResourceResult<()> {
        if self.ptr.is_none() {
            self.ptr = Some(context.map_allocation(self.allocation)?);
        }
        Ok(())
    }

    #[inline]
    pub fn unmap_memory(&mut self, context: &Context) -> ResourceResult<()> {
        if self.ptr.is_some() {
            context.unmap_allocation(self.allocation)?;
            self.ptr = None;
        }
        Ok(())
    }
}
