use std::{convert::Infallible, marker::PhantomData, ops::BitAndAssign};

use ash::vk;
use type_kit::{
    Cons, Contains, Create, CreateResult, Destroy, DestroyResult, FromGuard, GenVec, Marker, Nil,
    list_type, unpack_list,
};

use crate::{
    Context,
    memory::{
        AllocReqTyped, DeviceLocal, HostCoherent, HostVisible, MemoryProperties,
        allocator::{
            AllocReq, AllocationBorrow, AllocationStore, Allocator, AllocatorBuilder,
            AllocatorIndex, AllocatorIndexTyped, MemoryIndex, NoReleaseRange,
        },
        error::{MemoryError, MemoryResult},
        range::ByteRange,
    },
};

use super::AllocationIndexTyped;

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
    memory: MemoryIndex<M>,
}

impl<M: MemoryProperties> LinearBuffer<M> {
    fn allocate(
        &mut self,
        req: AllocReqTyped<M>,
        store: &mut AllocationStore<NoReleaseRange>,
    ) -> MemoryResult<AllocationIndexTyped<M>> {
        store.suballocate(req, self.memory)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LinearBufferBuilder<M: MemoryProperties> {
    info: BufferInfo,
    _phantom: PhantomData<M>,
}

impl<M: MemoryProperties> LinearBufferBuilder<M> {
    fn try_build(
        self,
        context: &Context,
        storage: &mut AllocationStore<NoReleaseRange>,
    ) -> MemoryResult<Option<LinearBuffer<M>>> {
        if !self.info.range.is_empty() && self.info.memory_type_bits != 0 {
            let req = M::alloc_req_typed(vk::MemoryRequirements {
                size: self.info.range.len() as vk::DeviceSize,
                memory_type_bits: self.info.memory_type_bits,
                ..Default::default()
            });
            let memory = storage.allocate(context, req)?;
            Ok(Some(LinearBuffer { memory }))
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

impl Default for StaticConfig {
    fn default() -> Self {
        Self::new()
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

#[derive(Debug)]
pub struct Static {
    buffers: Buffers,
    store: AllocationStore<NoReleaseRange>,
}

impl Create for Static {
    type Config<'a> = StaticConfig;
    type CreateError = MemoryError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let unpack_list![
            device_local_builder,
            host_coherent_builder,
            host_visible_builder
        ] = config.builders;
        let mut buffers: Buffers = Buffers::default();
        let mut store = AllocationStore::new();
        *buffers.get_mut::<Option<LinearBuffer<DeviceLocal>>, _>() =
            device_local_builder.try_build(context, &mut store)?;
        *buffers.get_mut::<Option<LinearBuffer<HostCoherent>>, _>() =
            host_coherent_builder.try_build(context, &mut store)?;
        *buffers.get_mut::<Option<LinearBuffer<HostVisible>>, _>() =
            host_visible_builder.try_build(context, &mut store)?;
        Ok(Self { buffers, store })
    }
}

impl Destroy for Static {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.store.destroy(context)?;
        Ok(())
    }
}

impl Static {
    #[inline]
    fn allocate_memory_type<M: MemoryProperties, T: Marker>(
        &mut self,
        req: AllocReqTyped<M>,
    ) -> MemoryResult<AllocationIndexTyped<M>>
    where
        Buffers: Contains<Option<LinearBuffer<M>>, T>,
    {
        self.buffers
            .get_mut::<Option<LinearBuffer<M>>, _>()
            .as_mut()
            .map(|buffer| buffer.allocate(req, &mut self.store))
            .ok_or(MemoryError::OutOfMemory)?
    }
}

impl Allocator for Static {
    type Storage = GenVec<Self>;

    #[inline]
    fn allocate<M: MemoryProperties>(
        &mut self,
        _context: &Context,
        req: AllocReqTyped<M>,
    ) -> MemoryResult<AllocationIndexTyped<M>> {
        let allocation = match req.into() {
            AllocReq::DeviceLocal(req) => self.allocate_memory_type(req)?.into_inner(),
            AllocReq::HostCoherent(req) => self.allocate_memory_type(req)?.into_inner(),
            AllocReq::HostVisible(req) => self.allocate_memory_type(req)?.into_inner(),
        };
        Ok(unsafe { AllocationIndexTyped::from_inner(allocation) })
    }

    #[inline]
    fn free<M: MemoryProperties>(
        &mut self,
        context: &Context,
        allocation: AllocationIndexTyped<M>,
    ) -> MemoryResult<()> {
        if let Some(mut memory) = self.store.pop(allocation)? {
            let _ = memory.destroy(context);
        }
        Ok(())
    }

    #[inline]
    fn borrow<M: MemoryProperties>(
        &mut self,
        allocation: AllocationIndexTyped<M>,
    ) -> MemoryResult<AllocationBorrow<M>> {
        self.store.borrow(allocation)
    }

    #[inline]
    fn put_back<'a, M: MemoryProperties>(
        &mut self,
        allocation: super::AllocationBorrow<M>,
    ) -> MemoryResult<()> {
        self.store.put_back(allocation)
    }

    #[inline]
    fn wrap_index(index: AllocatorIndexTyped<Self>) -> AllocatorIndex {
        AllocatorIndex::Static(index)
    }
}
