use std::{convert::Infallible, marker::PhantomData, ops::BitAndAssign, u32};

use ash::vk;
use type_kit::{
    list_type, unpack_list, Cons, Contains, Create, CreateResult, Destroy, DestroyResult,
    FromGuard, Marker, Nil,
};

use crate::{
    device::{
        memory::{AllocReqTyped, DeviceLocal, HostCoherent, HostVisible, MemoryProperties},
        raw::{
            allocator::{
                AllocReq, Allocation, AllocationStore, Allocator, AllocatorBuilder,
                AllocatorInstance,
            },
            range::ByteRange,
            resources::{memory::Memory, ResourceIndex},
        },
    },
    error::{AllocatorError, ResourceError, ResourceResult},
    Context,
};

use super::AllocationIndex;

#[derive(Debug, Clone, Copy)]
struct BufferInfo {
    range: ByteRange,
    memory_type_bits: u32,
}

impl Default for BufferInfo {
    fn default() -> Self {
        Self {
            range: ByteRange::empty(),
            memory_type_bits: u32::MAX,
        }
    }
}
#[derive(Debug, Clone, Copy)]
struct LinearBuffer<M: MemoryProperties> {
    memory: ResourceIndex<Memory<M>>,
    info: BufferInfo,
}

impl<M: MemoryProperties> LinearBuffer<M> {
    fn allocate(&mut self, req: AllocReqTyped<M>) -> ResourceResult<Allocation<M>> {
        let requirements = req.requirements();
        if (self.info.memory_type_bits & requirements.memory_type_bits) != 0 {
            let range = self
                .info
                .range
                .alloc_raw(requirements.size as usize, requirements.alignment as usize)
                .ok_or(AllocatorError::OutOfMemory)?;
            Ok(Allocation::new(self.memory, range))
        } else {
            Err(ResourceError::AllocatorError(
                AllocatorError::InvalidConfiguration,
            ))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LinearBufferBuilder<M: MemoryProperties> {
    info: BufferInfo,
    _phantom: PhantomData<M>,
}

impl<M: MemoryProperties> LinearBufferBuilder<M> {
    fn try_build(self, context: &Context) -> ResourceResult<Option<LinearBuffer<M>>> {
        if self.info.range.len() != 0 && self.info.memory_type_bits != 0 {
            let alloc_info =
                context.get_memory_allocate_info(M::alloc_req_typed(vk::MemoryRequirements {
                    size: self.info.range.len() as vk::DeviceSize,
                    memory_type_bits: self.info.memory_type_bits,
                    ..Default::default()
                }))?;
            let memory = context.create_resource(alloc_info)?;
            Ok(Some(LinearBuffer {
                memory,
                info: self.info,
            }))
        } else {
            Ok(None)
        }
    }
}

impl<M: MemoryProperties> Default for LinearBufferBuilder<M> {
    fn default() -> Self {
        Self {
            info: Default::default(),
            _phantom: PhantomData,
        }
    }
}

type BufferBuilders = list_type![
    LinearBufferBuilder<DeviceLocal>,
    LinearBufferBuilder<HostCoherent>,
    LinearBufferBuilder<HostVisible>,
    Nil
];

#[derive(Debug)]
pub struct StaticConfig {
    builders: BufferBuilders,
}

impl AllocatorBuilder for StaticConfig {
    #[inline]
    fn with_allocation<M: MemoryProperties>(&mut self, req: AllocReqTyped<M>) -> &mut Self {
        self.push_allocation(req)
    }
}

impl StaticConfig {
    #[inline]
    pub fn new() -> Self {
        Self {
            builders: Default::default(),
        }
    }

    #[inline]
    pub fn push_allocation_type<M: MemoryProperties, T: Marker>(
        &mut self,
        req: AllocReqTyped<M>,
    ) -> &mut Self
    where
        BufferBuilders: Contains<LinearBufferBuilder<M>, T>,
    {
        let requirements = req.requirements();
        let info = &mut self.builders.get_mut::<LinearBufferBuilder<M>, _>().info;
        info.range
            .extend_raw(requirements.size as usize, requirements.alignment as usize);
        info.memory_type_bits
            .bitand_assign(requirements.memory_type_bits);
        self
    }

    #[inline]
    pub fn push_allocation<R: Into<AllocReq>>(&mut self, req: R) -> &mut Self {
        match req.into() {
            AllocReq::DeviceLocal(req) => self.push_allocation_type(req),
            AllocReq::HostVisible(req) => self.push_allocation_type(req),
            AllocReq::HostCoherent(req) => self.push_allocation_type(req),
        }
    }
}

type Buffers = list_type![
    Option<LinearBuffer<DeviceLocal>>,
    Option<LinearBuffer<HostCoherent>>,
    Option<LinearBuffer<HostVisible>>,
    Nil
];

impl<M: MemoryProperties> Destroy for LinearBuffer<M> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = context.destroy_resource(self.memory);
        Ok(())
    }
}

#[derive(Debug)]
pub struct Static {
    buffers: Buffers,
    store: AllocationStore,
}

impl Create for Static {
    type Config<'a> = StaticConfig;
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let unpack_list![
            device_local_builder,
            host_coherent_builder,
            host_visible_builder,
            _nil
        ] = config.builders;
        let mut buffers: Buffers = Buffers::default();
        *buffers.get_mut::<Option<LinearBuffer<DeviceLocal>>, _>() =
            device_local_builder.try_build(context)?;
        *buffers.get_mut::<Option<LinearBuffer<HostCoherent>>, _>() =
            host_coherent_builder.try_build(context)?;
        *buffers.get_mut::<Option<LinearBuffer<HostVisible>>, _>() =
            host_visible_builder.try_build(context)?;
        Ok(Self {
            buffers,
            store: AllocationStore::new(),
        })
    }
}

impl Destroy for Static {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.buffers.destroy(context);
        Ok(())
    }
}

impl From<Static> for AllocatorInstance {
    #[inline]
    fn from(value: Static) -> Self {
        AllocatorInstance::Static(value)
    }
}

impl Static {
    #[inline]
    fn allocate_memory_type<M: MemoryProperties, T: Marker>(
        &mut self,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<Allocation<M>>
    where
        Buffers: Contains<Option<LinearBuffer<M>>, T>,
    {
        self.buffers
            .get_mut::<Option<LinearBuffer<M>>, _>()
            .as_mut()
            .map(|buffer| buffer.allocate(req))
            .ok_or(ResourceError::AllocatorError(
                AllocatorError::UnsupportedMemoryType,
            ))?
    }
}

impl Allocator for Static {
    #[inline]
    fn allocate<'a, M: MemoryProperties>(
        &mut self,
        _context: &Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationIndex<M>> {
        let allocation = match req.into() {
            AllocReq::DeviceLocal(req) => self.allocate_memory_type(req)?.into_inner(),
            AllocReq::HostCoherent(req) => self.allocate_memory_type(req)?.into_inner(),
            AllocReq::HostVisible(req) => self.allocate_memory_type(req)?.into_inner(),
        };
        Ok(self
            .store
            .push(unsafe { Allocation::<M>::from_inner(allocation) })?)
    }

    #[inline]
    fn free<'a, M: MemoryProperties>(
        &mut self,
        _context: &Context,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<()> {
        self.store.pop(allocation).map(|_| ())
    }

    #[inline]
    fn get_allocation<M: MemoryProperties>(
        &self,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<Allocation<M>> {
        self.store.get_allocation(allocation)
    }
}
