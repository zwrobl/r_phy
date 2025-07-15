use std::{fmt::Debug, marker::PhantomData};

use ash::{self, vk};

use crate::context::{
    error::{AllocatorError, AllocatorResult},
    Context,
};

#[derive(Debug, Clone, Copy)]
pub enum MemoryType {
    DeviceLocal,
    HostCoherent,
    HostVisible,
}

impl MemoryType {
    #[inline]
    pub fn get_property_flags(&self) -> vk::MemoryPropertyFlags {
        match self {
            MemoryType::DeviceLocal => DeviceLocal::properties(),
            MemoryType::HostCoherent => HostCoherent::properties(),
            MemoryType::HostVisible => HostVisible::properties(),
        }
    }

    #[inline]
    pub fn get_alloc_req(&self, requirements: vk::MemoryRequirements) -> AllocReq {
        match self {
            MemoryType::DeviceLocal => DeviceLocal::alloc_req(requirements),
            MemoryType::HostCoherent => HostCoherent::alloc_req(requirements),
            MemoryType::HostVisible => HostVisible::alloc_req(requirements),
        }
    }
}

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
        M::alloc_req(value.requirements())
    }
}

impl AllocReq {
    #[inline]
    pub fn requirements(&self) -> vk::MemoryRequirements {
        match self {
            AllocReq::HostVisible(req) => req.requirements,
            AllocReq::DeviceLocal(req) => req.requirements,
            AllocReq::HostCoherent(req) => req.requirements,
        }
    }

    #[inline]
    pub fn properties(&self) -> vk::MemoryPropertyFlags {
        match self {
            AllocReq::HostVisible(req) => req.properties(),
            AllocReq::DeviceLocal(req) => req.properties(),
            AllocReq::HostCoherent(req) => req.properties(),
        }
    }

    #[inline]
    pub fn get_memory_type(&self) -> MemoryType {
        match self {
            AllocReq::DeviceLocal(_) => MemoryType::DeviceLocal,
            AllocReq::HostCoherent(_) => MemoryType::HostCoherent,
            AllocReq::HostVisible(_) => MemoryType::HostVisible,
        }
    }
}

pub trait MemoryProperties: 'static + Sized {
    fn properties() -> vk::MemoryPropertyFlags;

    fn memory_type() -> MemoryType;

    fn alloc_req(requirements: vk::MemoryRequirements) -> AllocReq;

    #[inline]
    fn alloc_req_typed(requirements: vk::MemoryRequirements) -> AllocReqTyped<Self> {
        AllocReqTyped::<Self>::new(requirements)
    }
}

#[derive(Debug)]
pub struct HostVisible;

impl MemoryProperties for HostVisible {
    #[inline]
    fn properties() -> vk::MemoryPropertyFlags {
        vk::MemoryPropertyFlags::HOST_VISIBLE
    }

    #[inline]
    fn memory_type() -> MemoryType {
        MemoryType::HostVisible
    }

    #[inline]
    fn alloc_req(requirements: vk::MemoryRequirements) -> AllocReq {
        AllocReq::HostVisible(Self::alloc_req_typed(requirements))
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
    fn memory_type() -> MemoryType {
        MemoryType::HostCoherent
    }

    #[inline]
    fn alloc_req(requirements: vk::MemoryRequirements) -> AllocReq {
        AllocReq::HostCoherent(Self::alloc_req_typed(requirements))
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
    fn memory_type() -> MemoryType {
        MemoryType::DeviceLocal
    }

    #[inline]
    fn alloc_req(requirements: vk::MemoryRequirements) -> AllocReq {
        AllocReq::DeviceLocal(Self::alloc_req_typed(requirements))
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
    pub fn get_memory_type_index<R: Into<AllocReq>>(&self, req: R) -> AllocatorResult<u32> {
        let req: AllocReq = req.into();
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
