mod allocator;

use std::{
    any::type_name,
    ffi::c_void,
    fmt::{self, Debug, Formatter},
    marker::PhantomData,
    ops::Deref,
};

use ash::{self, vk};
use type_kit::{GenIndex, GuardIndex, TypeGuard};

use crate::context::device::raw::allocator::{Allocation, AllocationIndex};

use super::{resources::buffer::ByteRange, Device};

pub use allocator::*;

#[derive(Debug, Clone, Copy)]
pub struct MemoryTypeInfo {
    pub properties: vk::MemoryPropertyFlags,
    pub wrap_index: fn(GenIndex<TypeGuard<Allocation>>) -> AllocationIndex,
}

pub trait MemoryProperties: 'static + Sized {
    fn properties() -> vk::MemoryPropertyFlags;

    fn wrap_index(index: GuardIndex<Allocation>) -> AllocationIndex;

    #[inline]
    fn get_memory_type_info() -> MemoryTypeInfo {
        MemoryTypeInfo {
            properties: Self::properties(),
            wrap_index: Self::wrap_index,
        }
    }
}

#[derive(Debug)]
pub struct HostVisible;

impl MemoryProperties for HostVisible {
    fn properties() -> vk::MemoryPropertyFlags {
        vk::MemoryPropertyFlags::HOST_VISIBLE
    }

    #[inline]
    fn wrap_index(index: GuardIndex<Allocation>) -> AllocationIndex {
        AllocationIndex::HostVisible(index)
    }
}

#[derive(Debug)]
pub struct HostCoherent;

impl MemoryProperties for HostCoherent {
    fn properties() -> vk::MemoryPropertyFlags {
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT
    }

    #[inline]
    fn wrap_index(index: GuardIndex<Allocation>) -> AllocationIndex {
        AllocationIndex::HostCoherent(index)
    }
}

#[derive(Debug)]
pub struct DeviceLocal;

impl MemoryProperties for DeviceLocal {
    fn properties() -> vk::MemoryPropertyFlags {
        vk::MemoryPropertyFlags::DEVICE_LOCAL
    }

    #[inline]
    fn wrap_index(index: GuardIndex<Allocation>) -> AllocationIndex {
        AllocationIndex::DeviceLocal(index)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Resource {
    Buffer(vk::Buffer),
    Image(vk::Image),
}

impl From<vk::Image> for Resource {
    fn from(image: vk::Image) -> Self {
        Resource::Image(image)
    }
}

impl From<vk::Buffer> for Resource {
    fn from(buffer: vk::Buffer) -> Self {
        Resource::Buffer(buffer)
    }
}

impl Device {
    pub fn bind_memory<T: Into<Resource>, C: Memory>(
        &self,
        resource: T,
        memory: &C,
    ) -> Result<(), vk::Result> {
        let MemoryChunkRaw { memory, range } = *memory.chunk();

        match resource.into() {
            Resource::Buffer(buffer) => unsafe {
                self.bind_buffer_memory(buffer, memory, range.beg as vk::DeviceSize)
            },
            Resource::Image(image) => unsafe {
                self.bind_image_memory(image, memory, range.beg as vk::DeviceSize)
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryChunkRaw {
    memory: vk::DeviceMemory,
    range: ByteRange,
}

pub struct MemoryChunk<M: MemoryProperties> {
    raw: MemoryChunkRaw,
    _phantom: PhantomData<M>,
}

impl<M: MemoryProperties> Debug for MemoryChunk<M> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("MemoryChunk")
            .field("raw", &self.raw)
            .field("_phantom", &type_name::<M>())
            .finish()
    }
}

impl<M: MemoryProperties> Clone for MemoryChunk<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: MemoryProperties> Copy for MemoryChunk<M> {}

impl<M: MemoryProperties> MemoryChunk<M> {
    pub fn empty() -> Self {
        MemoryChunk {
            raw: MemoryChunkRaw {
                memory: vk::DeviceMemory::null(),
                range: ByteRange::new(0),
            },
            _phantom: PhantomData,
        }
    }
}

impl<M: MemoryProperties> Deref for MemoryChunk<M> {
    type Target = MemoryChunkRaw;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

pub trait Memory: 'static + Debug {
    type Properties: MemoryProperties;
    fn chunk(&self) -> MemoryChunk<Self::Properties>;
    fn map(&mut self, device: &ash::Device, range: ByteRange) -> Result<*mut c_void, vk::Result>;
    fn unmap(&mut self, device: &ash::Device);
}

impl<M: MemoryProperties> Memory for MemoryChunk<M> {
    type Properties = M;
    fn chunk(&self) -> MemoryChunk<Self::Properties> {
        *self
    }

    fn map(&mut self, device: &ash::Device, range: ByteRange) -> Result<*mut c_void, vk::Result> {
        // TODO: Add checks for valid memory properties, consinder panic or returning dedicated result type
        unsafe {
            device.map_memory(
                self.memory,
                (self.range.beg + range.beg) as vk::DeviceSize,
                range.len() as vk::DeviceSize,
                vk::MemoryMapFlags::empty(),
            )
        }
    }

    fn unmap(&mut self, device: &ash::Device) {
        unsafe {
            device.unmap_memory(self.memory);
        }
    }
}
