mod page;
mod r#static;
mod unpooled;

pub use page::*;
pub use r#static::*;
pub use unpooled::*;

use std::{collections::HashMap, convert::Infallible, ffi::c_void, fmt::Debug};

use ash::vk;
use type_kit::{
    Create, Destroy, DestroyResult, DropGuardError, FromGuard, GenCollection, GenIndex,
    GenIndexRaw, GuardCollection, GuardIndex, TypeGuard, TypeGuardCollection,
};

use crate::{
    device::{
        memory::{
            AllocReq, AllocReqTyped, BindResource, DeviceLocal, HostCoherent, HostVisible,
            MemoryProperties, MemoryType,
        },
        raw::{
            range::ByteRange,
            resources::{
                memory::{Memory, MemoryRaw},
                ResourceIndex,
            },
        },
    },
    error::{AllocatorError, ResourceError, ResourceResult},
    Context,
};

#[derive(Debug, Clone, Copy)]
pub struct AllocationRaw {
    range: ByteRange,
    memory: GenIndexRaw,
}

#[derive(Debug, Clone, Copy)]
pub struct Allocation<M: MemoryProperties> {
    range: ByteRange,
    memory: ResourceIndex<Memory<M>>,
}

impl<M: MemoryProperties> Allocation<M> {
    #[inline]
    pub fn new(memory: ResourceIndex<Memory<M>>, range: ByteRange) -> Self {
        Self { range, memory }
    }

    pub unsafe fn cast<T: MemoryProperties>(self) -> Allocation<T> {
        Allocation {
            range: self.range,
            memory: ResourceIndex::<Memory<T>>::from_inner(self.memory.into_inner()),
        }
    }
}

impl<M: MemoryProperties> FromGuard for Allocation<M> {
    type Inner = AllocationRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        AllocationRaw {
            range: self.range,
            memory: self.memory.into_inner(),
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        let inner: AllocationRaw = inner;
        Self {
            range: inner.range,
            memory: ResourceIndex::<Memory<M>>::from_inner(inner.memory),
        }
    }
}

#[derive(Debug, Default)]
struct MemoryMap {
    usage: HashMap<TypeGuard<GenIndexRaw>, usize>,
    _memory: GuardCollection<MemoryRaw>,
}

impl MemoryMap {
    #[inline]
    fn new() -> Self {
        Self {
            usage: HashMap::default(),
            _memory: GuardCollection::default(),
        }
    }

    #[inline]
    fn register<M: MemoryProperties>(&mut self, allocation: &Allocation<M>) {
        let memory = allocation.memory.into_guard();
        *self.usage.entry(memory).or_default() += 1;
    }

    #[inline]
    fn pop<M: MemoryProperties>(
        &mut self,
        allocation: Allocation<M>,
    ) -> ResourceResult<Option<ResourceIndex<Memory<M>>>> {
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

    fn drain<M: MemoryProperties>(&mut self) -> Vec<ResourceIndex<Memory<M>>> {
        let (valid, rest): (Vec<_>, Vec<_>) = self
            .usage
            .drain()
            .map(|(memory, count)| {
                ResourceIndex::<Memory<M>>::try_from_guard(memory)
                    .map_err(|(memory, _)| (memory, count))
            })
            .partition(Result::is_ok);
        self.usage = rest.into_iter().map(Result::unwrap_err).collect();
        valid.into_iter().map(Result::unwrap).collect()
    }

    #[inline]
    fn free_memory(&mut self, context: &Context) {
        self.drain::<DeviceLocal>().into_iter().for_each(|memory| {
            context.destroy_resource(memory).unwrap();
        });
        self.drain::<HostCoherent>().into_iter().for_each(|memory| {
            context.destroy_resource(memory).unwrap();
        });
        self.drain::<HostVisible>().into_iter().for_each(|memory| {
            context.destroy_resource(memory).unwrap();
        });
    }
}

impl Drop for MemoryMap {
    #[inline]
    fn drop(&mut self) {
        assert!(self.usage.is_empty());
    }
}

#[derive(Debug)]
pub struct AllocationStore {
    allocations: TypeGuardCollection<AllocationRaw>,
    memory_map: MemoryMap,
}

impl AllocationStore {
    #[inline]
    pub fn new() -> Self {
        Self {
            allocations: TypeGuardCollection::default(),
            memory_map: MemoryMap::new(),
        }
    }

    #[inline]
    pub fn push<M: MemoryProperties>(
        &mut self,
        allocation: Allocation<M>,
    ) -> ResourceResult<AllocationIndex<M>> {
        self.memory_map.register(&allocation);
        let index = self.allocations.push(allocation.into_guard())?;
        Ok(AllocationIndex { index })
    }

    #[inline]
    pub fn pop<M: MemoryProperties>(
        &mut self,
        index: AllocationIndex<M>,
    ) -> ResourceResult<Option<ResourceIndex<Memory<M>>>> {
        let allocation = Allocation::try_from_guard(self.allocations.pop(index.index)?).unwrap();
        self.memory_map.pop(allocation)
    }

    #[inline]
    pub fn get_allocation<M: MemoryProperties>(
        &self,
        index: AllocationIndex<M>,
    ) -> ResourceResult<Allocation<M>> {
        let allocation = Allocation::try_from_guard(*self.allocations.get(index.index)?).unwrap();
        Ok(allocation)
    }
}

impl Destroy for AllocationStore {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.memory_map.free_memory(context);
        Ok(())
    }
}

pub trait AllocatorBuilder {
    fn with_allocation<M: MemoryProperties>(&mut self, req: AllocReqTyped<M>) -> &mut Self;
}

pub trait Allocator: 'static + Sized
where
    for<'a> Self: Destroy<Context<'a> = &'a Context, DestroyError = Infallible>
        + Create<CreateError = ResourceError>
        + Into<AllocatorInstance>,
{
    fn allocate<'a, M: MemoryProperties>(
        &mut self,
        context: &Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationIndex<M>>;

    fn free<'a, M: MemoryProperties>(
        &mut self,
        context: &Context,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<()>;

    fn get_allocation<M: MemoryProperties>(
        &self,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<Allocation<M>>;
}

#[derive(Debug)]
pub enum AllocatorInstance {
    Page(Page),
    Unpooled(Unpooled),
    Static(Static),
}

impl AllocatorInstance {
    #[inline]
    pub fn allocate<M: MemoryProperties>(
        &mut self,
        context: &Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationIndex<M>> {
        match self {
            AllocatorInstance::Page(allocator) => allocator.allocate(context, req),
            AllocatorInstance::Unpooled(allocator) => allocator.allocate(context, req),
            AllocatorInstance::Static(allocator) => allocator.allocate(context, req),
        }
    }

    #[inline]
    pub fn free<M: MemoryProperties>(
        &mut self,
        context: &Context,
        index: AllocationIndex<M>,
    ) -> ResourceResult<()> {
        match self {
            AllocatorInstance::Page(allocator) => allocator.free(context, index),
            AllocatorInstance::Unpooled(allocator) => allocator.free(context, index),
            AllocatorInstance::Static(allocator) => allocator.free(context, index),
        }
    }

    #[inline]
    pub fn get_allocation<M: MemoryProperties>(
        &self,
        index: AllocationIndex<M>,
    ) -> ResourceResult<Allocation<M>> {
        match self {
            AllocatorInstance::Page(allocator) => allocator.get_allocation(index),
            AllocatorInstance::Unpooled(allocator) => allocator.get_allocation(index),
            AllocatorInstance::Static(allocator) => allocator.get_allocation(index),
        }
    }
}

impl Destroy for AllocatorInstance {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        match self {
            AllocatorInstance::Page(allocator) => allocator.destroy(context),
            AllocatorInstance::Unpooled(allocator) => allocator.destroy(context),
            AllocatorInstance::Static(allocator) => allocator.destroy(context),
        }
    }
}

impl Context {
    pub fn map_allocation<M: MemoryProperties>(
        &self,
        allocation: AllocationEntry<M>,
    ) -> ResourceResult<*mut c_void> {
        let AllocationEntry {
            allocation,
            allocator,
        } = allocation;
        let storage = self.allocators.borrow();
        let Allocation { memory, range } = storage
            .allocators
            .get(allocator.index)?
            .get_allocation(allocation)?;
        let mut storage = self.storage.borrow_mut();
        let mut memory = storage.entry_mut(memory)?;
        let ptr = unsafe { memory.map(self)?.byte_offset(range.beg as isize) };
        Ok(ptr)
    }

    pub fn unmap_allocation<M: MemoryProperties>(
        &self,
        allocation: AllocationEntry<M>,
    ) -> ResourceResult<()> {
        let AllocationEntry {
            allocation,
            allocator,
        } = allocation;
        let storage = self.allocators.borrow();
        let Allocation { memory, .. } = storage
            .allocators
            .get(allocator.index)?
            .get_allocation(allocation)?;
        let mut storage = self.storage.borrow_mut();
        storage.entry_mut(memory)?.unmap(self);
        Ok(())
    }

    pub fn bind_memory<R: Into<BindResource>, M: MemoryProperties>(
        &self,
        resource: R,
        allocation: AllocationEntry<M>,
    ) -> ResourceResult<()> {
        let AllocationEntry {
            allocation,
            allocator,
        } = allocation;
        let storage = self.allocators.borrow();
        let Allocation { memory, range } = storage
            .allocators
            .get(allocator.index)?
            .get_allocation(allocation)?;
        let storage = self.storage.borrow();
        let memory = storage.entry(memory)?;
        match resource.into() {
            BindResource::Image(image) => unsafe {
                self.bind_image_memory(image, **memory, range.beg as vk::DeviceSize)
            },
            BindResource::Buffer(buffer) => unsafe {
                self.bind_buffer_memory(buffer, **memory, range.beg as vk::DeviceSize)
            },
        }?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AllocatorIndex {
    index: GenIndex<AllocatorInstance, GenCollection<AllocatorInstance>>,
}

#[derive(Debug)]
pub struct AllocationIndex<M: MemoryProperties> {
    index: GuardIndex<Allocation<M>, TypeGuardCollection<AllocationRaw>>,
}

impl<M: MemoryProperties> Clone for AllocationIndex<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: MemoryProperties> Copy for AllocationIndex<M> {}

#[derive(Debug, Clone, Copy)]
pub struct AllocationIndexRaw {
    index: GenIndexRaw,
}

impl<M: MemoryProperties> FromGuard for AllocationIndex<M> {
    type Inner = AllocationIndexRaw;

    fn into_inner(self) -> Self::Inner {
        AllocationIndexRaw {
            index: self.index.into_inner(),
        }
    }

    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            index: GuardIndex::<Allocation<M>, _>::from_inner(inner.index),
        }
    }
}

#[derive(Debug)]
pub struct AllocationEntry<M: MemoryProperties> {
    allocator: AllocatorIndex,
    allocation: AllocationIndex<M>,
}

impl<M: MemoryProperties> Clone for AllocationEntry<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: MemoryProperties> Copy for AllocationEntry<M> {}

#[derive(Debug, Clone, Copy)]
pub struct AllocationEntryRaw {
    memory_type: MemoryType,
    allocator: AllocatorIndex,
    allocation: AllocationIndexRaw,
}

impl<M: MemoryProperties> FromGuard for AllocationEntry<M> {
    type Inner = AllocationEntryRaw;

    fn into_inner(self) -> Self::Inner {
        AllocationEntryRaw {
            memory_type: M::memory_type(),
            allocator: self.allocator,
            allocation: self.allocation.into_inner(),
        }
    }

    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            allocator: inner.allocator,
            allocation: AllocationIndex::<M>::from_inner(inner.allocation),
        }
    }
}

#[derive(Debug)]
pub enum AllocationEntryTyped {
    DeviceLocal(AllocationEntry<DeviceLocal>),
    HostVisible(AllocationEntry<HostVisible>),
    HostCoherent(AllocationEntry<HostCoherent>),
}

impl AllocationEntryRaw {
    #[inline]
    pub fn into_typed(self) -> AllocationEntryTyped {
        match self.memory_type {
            MemoryType::DeviceLocal => AllocationEntryTyped::DeviceLocal(unsafe {
                AllocationEntry::<DeviceLocal>::from_inner(self)
            }),
            MemoryType::HostCoherent => AllocationEntryTyped::HostCoherent(unsafe {
                AllocationEntry::<HostCoherent>::from_inner(self)
            }),
            MemoryType::HostVisible => AllocationEntryTyped::HostVisible(unsafe {
                AllocationEntry::<HostVisible>::from_inner(self)
            }),
        }
    }
}

impl Context {
    #[inline]
    pub(crate) fn free_allocation_raw(&self, allocation: AllocationEntryRaw) -> ResourceResult<()> {
        match allocation.into_typed() {
            AllocationEntryTyped::DeviceLocal(allocation) => self.free(allocation),
            AllocationEntryTyped::HostVisible(allocation) => self.free(allocation),
            AllocationEntryTyped::HostCoherent(allocation) => self.free(allocation),
        }
    }
}

pub struct AllocatorStorage {
    allocators: GenCollection<AllocatorInstance>,
}

impl AllocatorStorage {
    #[inline]
    pub fn new() -> Self {
        Self {
            allocators: GenCollection::new(),
        }
    }

    #[inline]
    pub fn create_allocator<'a, 'b, A: Allocator>(
        &mut self,
        context: &'a Context,
        config: A::Config<'a>,
    ) -> ResourceResult<AllocatorIndex> {
        let allocator = A::create(config, context)?.into();
        let index = self.allocators.push(allocator)?;
        Ok(AllocatorIndex { index })
    }

    #[inline]
    pub fn destroy_allocator(
        &mut self,
        context: &Context,
        index: AllocatorIndex,
    ) -> ResourceResult<()> {
        let _ = self.allocators.pop(index.index)?.destroy(context);
        Ok(())
    }

    #[inline]
    pub fn allocate<M: MemoryProperties>(
        &mut self,
        context: &Context,
        index: AllocatorIndex,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationEntry<M>> {
        let allocation = self
            .allocators
            .get_mut(index.index)?
            .allocate(context, req)?;
        let entry = AllocationEntry {
            allocator: index,
            allocation,
        };
        Ok(entry)
    }

    #[inline]
    pub fn free<M: MemoryProperties>(
        &mut self,
        context: &Context,
        index: AllocationEntry<M>,
    ) -> ResourceResult<()> {
        let AllocationEntry {
            allocator,
            allocation,
        } = index;
        self.allocators
            .get_mut(allocator.index)?
            .free(context, allocation)
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
