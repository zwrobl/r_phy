pub mod allocator;
pub mod range;

use std::{convert::Infallible, ffi::c_void, fmt::Debug, marker::PhantomData, ops::Deref};

use ash::{self, vk};
use type_kit::{Create, Destroy, DestroyResult, FromGuard};

use crate::{
    error::{AllocatorError, AllocatorResult, ResourceError, VkResult},
    memory::{
        allocator::{AllocationIndex, AllocationIndexTyped},
        range::ByteRange,
    },
    Context,
};

#[derive(Debug)]
pub struct AllocReqTyped<T: MemoryProperties> {
    requirements: vk::MemoryRequirements,
    _phantom: PhantomData<T>,
}

impl<T: MemoryProperties> Clone for AllocReqTyped<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: MemoryProperties> Copy for AllocReqTyped<T> {}

impl<M: MemoryProperties> AllocReqTyped<M> {
    #[inline]
    pub fn new(requirements: vk::MemoryRequirements) -> Self {
        Self {
            requirements,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn requirements(&self) -> vk::MemoryRequirements {
        self.requirements
    }

    #[inline]
    pub fn properties(&self) -> vk::MemoryPropertyFlags {
        M::properties()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AllocReq {
    HostVisible(AllocReqTyped<HostVisible>),
    DeviceLocal(AllocReqTyped<DeviceLocal>),
    HostCoherent(AllocReqTyped<HostCoherent>),
}

impl<M: MemoryProperties> From<AllocReqTyped<M>> for AllocReq {
    #[inline]
    fn from(value: AllocReqTyped<M>) -> AllocReq {
        M::wrap_req(value)
    }
}

pub trait MemoryProperties: 'static + Sized + Debug {
    fn properties() -> vk::MemoryPropertyFlags;

    fn wrap_req(req: AllocReqTyped<Self>) -> AllocReq;

    #[inline]
    fn alloc_req_typed(requirements: vk::MemoryRequirements) -> AllocReqTyped<Self> {
        AllocReqTyped::<Self>::new(requirements)
    }

    fn wrap_index(index: AllocationIndexTyped<Self>) -> AllocationIndex;
}

#[derive(Debug)]
pub struct HostVisible;

impl MemoryProperties for HostVisible {
    #[inline]
    fn properties() -> vk::MemoryPropertyFlags {
        vk::MemoryPropertyFlags::HOST_VISIBLE
    }

    #[inline]
    fn wrap_req(req: AllocReqTyped<Self>) -> AllocReq {
        AllocReq::HostVisible(req)
    }

    #[inline]
    fn wrap_index(index: AllocationIndexTyped<Self>) -> AllocationIndex {
        AllocationIndex::HostVisible(index)
    }
}

#[derive(Debug)]
pub struct HostCoherent;

impl MemoryProperties for HostCoherent {
    #[inline]
    fn properties() -> vk::MemoryPropertyFlags {
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT
    }

    #[inline]
    fn wrap_req(req: AllocReqTyped<Self>) -> AllocReq {
        AllocReq::HostCoherent(req)
    }

    #[inline]
    fn wrap_index(index: AllocationIndexTyped<Self>) -> AllocationIndex {
        AllocationIndex::HostCoherent(index)
    }
}

#[derive(Debug)]
pub struct DeviceLocal;

impl MemoryProperties for DeviceLocal {
    #[inline]
    fn properties() -> vk::MemoryPropertyFlags {
        vk::MemoryPropertyFlags::DEVICE_LOCAL
    }

    #[inline]
    fn wrap_req(req: AllocReqTyped<Self>) -> AllocReq {
        AllocReq::DeviceLocal(req)
    }

    #[inline]
    fn wrap_index(index: AllocationIndexTyped<Self>) -> AllocationIndex {
        AllocationIndex::DeviceLocal(index)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BindResource {
    Image(vk::Image),
    Buffer(vk::Buffer),
}

impl BindResource {
    #[inline]
    pub fn new<R: Into<BindResource>>(resource: R) -> Self {
        resource.into()
    }

    #[inline]
    pub fn get_alloc_req<M: MemoryProperties>(&self, context: &Context) -> AllocReqTyped<M> {
        M::alloc_req_typed(context.get_memory_requirements(*self))
    }
}

impl From<vk::Image> for BindResource {
    #[inline]
    fn from(value: vk::Image) -> Self {
        Self::Image(value)
    }
}

impl From<vk::Buffer> for BindResource {
    #[inline]
    fn from(value: vk::Buffer) -> Self {
        Self::Buffer(value)
    }
}

impl Context {
    #[inline]
    pub fn get_memory_requirements<T: Into<BindResource>>(
        &self,
        resource: T,
    ) -> vk::MemoryRequirements {
        match resource.into() {
            BindResource::Buffer(buffer) => unsafe { self.get_buffer_memory_requirements(buffer) },
            BindResource::Image(image) => unsafe { self.get_image_memory_requirements(image) },
        }
    }

    #[inline]
    pub fn get_memory_type_index<M: MemoryProperties>(
        &self,
        req: AllocReqTyped<M>,
    ) -> AllocatorResult<u32> {
        let memory_type_bits = req.requirements().memory_type_bits;
        let memory_properties = req.properties();

        self.physical_device
            .properties
            .memory
            .memory_types
            .iter()
            .zip(0u32..)
            .find_map(|(memory, type_index)| {
                if (1 << type_index & memory_type_bits == 1 << type_index)
                    && memory.property_flags.contains(memory_properties)
                {
                    Some(type_index)
                } else {
                    None
                }
            })
            .ok_or(AllocatorError::UnsupportedMemoryType)
    }
}

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
    range: ByteRange,
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
                range: ByteRange::new(info.allocation_size as usize),
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
    #[inline]
    pub fn suballocate(&mut self, size: usize, alignment: usize) -> Option<ByteRange> {
        self.memory.range.alloc_raw(size, alignment)
    }

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
