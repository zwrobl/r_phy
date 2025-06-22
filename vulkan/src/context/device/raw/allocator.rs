mod linear;
mod page;
mod unpooled;

pub use linear::*;
pub use page::*;
pub use unpooled::*;

use std::{collections::HashMap, convert::Infallible, fmt::Debug, marker::PhantomData};

use ash::vk;
use type_kit::{
    Create, CreateResult, Destroy, DestroyResult, DropGuardError, FromGuard, GenIndex, GenIndexRaw,
    GuardCollection, GuardIndex, ScopedEntryResult, ScopedInnerMut, TypeGuard, TypeGuardCollection,
    TypedIndex, Valid,
};

use crate::context::{
    device::{
        memory::{DeviceLocal, HostCoherent, HostVisible, MemoryProperties, MemoryTypeInfo},
        resources::buffer::ByteRange,
    },
    error::{AllocatorError, AllocatorResult, ResourceResult},
    Context,
};

use super::resources::{memory::Memory, ResourceIndex};

#[derive(Debug, Clone, Copy)]
pub struct Allocation {
    range: ByteRange,
    memory: ResourceIndex<Memory>,
}

impl Allocation {
    #[inline]
    pub fn new(memory: ResourceIndex<Memory>, range: ByteRange) -> Self {
        Self { range, memory }
    }
}

impl From<Valid<Allocation>> for Allocation {
    #[inline]
    fn from(value: Valid<Allocation>) -> Self {
        let allocation: Allocation = value.into_inner();
        allocation
    }
}

impl FromGuard for Allocation {
    type Inner = Allocation;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self
    }
}

#[derive(Debug, Default)]
struct MemoryMap {
    usage: HashMap<TypeGuard<GenIndexRaw>, usize>,
    memory: GuardCollection<Memory>,
}

impl MemoryMap {
    #[inline]
    fn new() -> Self {
        Self {
            usage: HashMap::default(),
            memory: GuardCollection::default(),
        }
    }

    #[inline]
    fn register(&mut self, allocation: &Allocation) {
        let memory = allocation.memory.clone().into_guard();
        *self.usage.entry(memory).or_default() += 1;
    }

    #[inline]
    fn pop(&mut self, allocation: Allocation) -> AllocatorResult<Option<ResourceIndex<Memory>>> {
        let memory = allocation.memory.clone().into_guard();
        let count = self
            .usage
            .get_mut(&memory)
            .ok_or(AllocatorError::InvalidAllocationIndex)?;
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.usage.remove(&memory);
            Ok(Some(allocation.memory))
        } else {
            Ok(None)
        }
    }

    fn drain(&mut self) -> Vec<ResourceIndex<Memory>> {
        let (valid, rest): (Vec<_>, Vec<_>) = self
            .usage
            .drain()
            .map(|(memory, count)| {
                ResourceIndex::<Memory>::try_from_guard(memory)
                    .map_err(|(memory, _)| (memory, count))
            })
            .partition(Result::is_ok);
        self.usage = rest.into_iter().map(Result::unwrap_err).collect();
        valid.into_iter().map(Result::unwrap).collect()
    }

    #[inline]
    fn free_memory(&mut self, context: &Context) {
        for memory in self.drain().into_iter() {
            context.destroy_resource(memory).unwrap();
        }
    }
}

impl Drop for MemoryMap {
    #[inline]
    fn drop(&mut self) {
        assert!(self.usage.is_empty());
    }
}

#[derive(Debug)]
pub enum AllocatorState {
    Empty(()),
    Page(PageState),
    Linear(LinearState),
}

impl From<()> for AllocatorState {
    fn from(config: ()) -> Self {
        Self::Empty(config)
    }
}

impl State for () {
    fn try_get(config: &AllocatorState) -> Result<&Self, AllocatorError> {
        match config {
            AllocatorState::Empty(empty) => Ok(&empty),
            _ => Err(AllocatorError::InvalidConfiguration),
        }
    }
}

pub struct Allocator<S: Strategy> {
    inner: AllocatorInner,
    _phantom: PhantomData<S>,
}

pub trait State: Into<AllocatorState> {
    fn try_get(config: &AllocatorState) -> Result<&Self, AllocatorError>;
}

#[derive(Debug)]
pub struct AllocatorInner {
    allocations: TypeGuardCollection<Allocation>,
    memory_map: MemoryMap,
    state: AllocatorState,
}

impl AllocatorInner {
    #[inline]
    pub fn new<S: Strategy>(state: S::State) -> Self {
        Self {
            allocations: TypeGuardCollection::default(),
            memory_map: MemoryMap::new(),
            state: state.into(),
        }
    }
}

impl Destroy for AllocatorInner {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.memory_map.free_memory(context);
        Ok(())
    }
}

impl<S: Strategy> From<Valid<Allocator<S>>> for Allocator<S> {
    fn from(value: Valid<Allocator<S>>) -> Self {
        Self {
            inner: value.into_inner(),
            _phantom: PhantomData,
        }
    }
}

impl<S: Strategy> FromGuard for Allocator<S> {
    type Inner = AllocatorInner;

    fn into_inner(self) -> Self::Inner {
        self.inner
    }
}

pub struct AllocationRequest {
    requirements: vk::MemoryRequirements,
    pub memory_type_info: MemoryTypeInfo,
}

impl AllocationRequest {
    #[inline]
    pub fn new(memory_type_info: MemoryTypeInfo, requirements: vk::MemoryRequirements) -> Self {
        Self {
            requirements,
            memory_type_info,
        }
    }
}

pub trait Strategy: 'static + Sized {
    type State: State;
    type CreateConfig<'a>: Into<Self::State>;

    fn wrap_index(index: GuardIndex<Allocator<Self>>) -> AllocatorIndex;

    fn allocate<'a>(
        allocator: ScopedInnerMut<'a, Allocator<Self>>,
        context: &Context,
        req: AllocationRequest,
    ) -> ResourceResult<AllocationIndex>;

    fn free<'a>(
        allocator: ScopedInnerMut<'a, Allocator<Self>>,
        context: &Context,
        allocation: AllocationIndex,
    ) -> ResourceResult<()>;
}

impl<S: Strategy> Allocator<S> {
    #[inline]
    pub fn new(config: S::State) -> Self {
        Self {
            inner: AllocatorInner::new::<S>(config),
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn access<'a, M: MemoryProperties>(
        &'a self,
        index: AllocationIndex,
    ) -> ScopedEntryResult<'a, Allocation> {
        self.inner
            .allocations
            .entry(TypedIndex::<Allocation>::new(index.into_inner()))
    }
}

impl<S: Strategy> Create for Allocator<S> {
    type Config<'a> = S::CreateConfig<'a>;
    type CreateError = AllocatorError;

    fn create<'a, 'b>(config: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
        Ok(Self::new(config.into()))
    }
}

impl<S: Strategy> Destroy for Allocator<S> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.inner.destroy(context)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BindResource {
    Image(vk::Image),
    Buffer(vk::Buffer),
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
    pub fn get_memory_type_index(&self, req: &AllocationRequest) -> AllocatorResult<u32> {
        let memory_type_bits = req.requirements.memory_type_bits;
        let memory_properties = req.memory_type_info.properties;

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
pub enum AllocatorIndex {
    Linear(GuardIndex<Allocator<Linear>>),
    Page(GuardIndex<Allocator<Page>>),
    Unpooled(GuardIndex<Allocator<Unpooled>>),
}

impl AllocatorIndex {
    #[inline]
    pub fn into_inner(self) -> GenIndex<TypeGuard<AllocatorInner>> {
        match self {
            AllocatorIndex::Linear(index) => index,
            AllocatorIndex::Page(index) => index,
            AllocatorIndex::Unpooled(index) => index,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AllocationIndex {
    HostCoherent(GuardIndex<Allocation>),
    HostVisible(GuardIndex<Allocation>),
    DeviceLocal(GuardIndex<Allocation>),
}

impl AllocationIndex {
    #[inline]
    pub fn into_inner(self) -> GuardIndex<Allocation> {
        match self {
            AllocationIndex::HostCoherent(index) => index,
            AllocationIndex::HostVisible(index) => index,
            AllocationIndex::DeviceLocal(index) => index,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AllocationEntry {
    allocator: AllocatorIndex,
    allocation: AllocationIndex,
}

pub struct AllocatorStorage {
    allocators: GuardCollection<AllocatorInner>,
}

impl AllocatorStorage {
    #[inline]
    pub fn new() -> Self {
        Self {
            allocators: GuardCollection::default(),
        }
    }

    #[inline]
    pub fn create_allocator<'a, 'b, S: Strategy>(
        &mut self,
        context: &'a Context,
        config: S::CreateConfig<'b>,
    ) -> ResourceResult<AllocatorIndex> {
        let allocator = Allocator::<S>::create(config, context)?;
        let index = self.allocators.push(allocator.into_guard())?;
        Ok(S::wrap_index(index))
    }

    #[inline]
    pub fn destroy_allocator(
        &mut self,
        context: &Context,
        index: AllocatorIndex,
    ) -> ResourceResult<()> {
        let _ = self.allocators.pop(index.into_inner())?.destroy(context);
        Ok(())
    }

    #[inline]
    pub fn allocate(
        &mut self,
        context: &Context,
        index: AllocatorIndex,
        req: AllocationRequest,
    ) -> ResourceResult<AllocationEntry> {
        let allocation = match index {
            AllocatorIndex::Linear(_) => {
                Linear::allocate(self.allocators.inner_mut(index.into_inner())?, context, req)
            }
            AllocatorIndex::Page(_) => {
                Page::allocate(self.allocators.inner_mut(index.into_inner())?, context, req)
            }
            AllocatorIndex::Unpooled(_) => {
                Unpooled::allocate(self.allocators.inner_mut(index.into_inner())?, context, req)
            }
        }?;
        let entry = AllocationEntry {
            allocator: index,
            allocation,
        };
        Ok(entry)
    }

    #[inline]
    pub fn free(&mut self, context: &Context, index: AllocationEntry) -> ResourceResult<()> {
        let AllocationEntry {
            allocator,
            allocation,
        } = index;
        match allocator {
            AllocatorIndex::Linear(index) => {
                Linear::free(self.allocators.inner_mut(index)?, context, allocation)
            }
            AllocatorIndex::Page(index) => {
                Page::free(self.allocators.inner_mut(index)?, context, allocation)
            }
            AllocatorIndex::Unpooled(index) => {
                Unpooled::free(self.allocators.inner_mut(index)?, context, allocation)
            }
        }?;
        Ok(())
    }
}

impl Destroy for AllocatorStorage {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.allocators.destroy(context)?;
        Ok(())
    }
}
