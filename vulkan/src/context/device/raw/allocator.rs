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
    GenIndexRaw, GuardCollection, GuardIndex, TypeGuard, TypeGuardCollection, Valid,
};

use crate::context::{
    device::{
        memory::{AllocReq, BindResource},
        raw::resources::{memory::Memory, ResourceIndex},
        resources::buffer::ByteRange,
    },
    error::{AllocatorError, ResourceError, ResourceResult},
    Context,
};

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
    _memory: GuardCollection<Memory>,
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
    fn register(&mut self, allocation: &Allocation) {
        let memory = allocation.memory.clone().into_guard();
        *self.usage.entry(memory).or_default() += 1;
    }

    #[inline]
    fn pop(&mut self, allocation: Allocation) -> ResourceResult<Option<ResourceIndex<Memory>>> {
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
pub struct AllocationStore {
    allocations: TypeGuardCollection<Allocation>,
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
    pub fn push(&mut self, allocation: Allocation) -> ResourceResult<AllocationIndex> {
        self.memory_map.register(&allocation);
        let index = self.allocations.push(allocation.into_guard())?;
        Ok(AllocationIndex { index })
    }

    #[inline]
    pub fn pop(&mut self, index: AllocationIndex) -> ResourceResult<Option<ResourceIndex<Memory>>> {
        let allocation = Allocation::try_from_guard(self.allocations.pop(index.index)?).unwrap();
        self.memory_map.pop(allocation)
    }

    #[inline]
    pub fn get_allocation(&self, index: AllocationIndex) -> ResourceResult<Allocation> {
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

pub trait Allocator: 'static + Sized
where
    for<'a> Self: Destroy<Context<'a> = &'a Context, DestroyError = Infallible>
        + Create<CreateError = ResourceError>
        + Into<AllocatorInstance>,
{
    fn allocate<'a>(&mut self, context: &Context, req: AllocReq)
        -> ResourceResult<AllocationIndex>;

    fn free<'a>(&mut self, context: &Context, allocation: AllocationIndex) -> ResourceResult<()>;

    fn get_allocation(&self, allocation: AllocationIndex) -> ResourceResult<Allocation>;
}

#[derive(Debug)]
pub enum AllocatorInstance {
    Page(Page),
    Unpooled(Unpooled),
    Static(Static),
}

impl AllocatorInstance {
    #[inline]
    pub fn allocate(
        &mut self,
        context: &Context,
        req: AllocReq,
    ) -> ResourceResult<AllocationIndex> {
        match self {
            AllocatorInstance::Page(allocator) => allocator.allocate(context, req),
            AllocatorInstance::Unpooled(allocator) => allocator.allocate(context, req),
            AllocatorInstance::Static(allocator) => allocator.allocate(context, req),
        }
    }

    #[inline]
    pub fn free(&mut self, context: &Context, index: AllocationIndex) -> ResourceResult<()> {
        match self {
            AllocatorInstance::Page(allocator) => allocator.free(context, index),
            AllocatorInstance::Unpooled(allocator) => allocator.free(context, index),
            AllocatorInstance::Static(allocator) => allocator.free(context, index),
        }
    }

    #[inline]
    pub fn get_allocation(&self, index: AllocationIndex) -> ResourceResult<Allocation> {
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
    pub fn map_allocation(&self, allocation: AllocationEntry) -> ResourceResult<*mut c_void> {
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

    pub fn unmap_allocation(&self, allocation: AllocationEntry) -> ResourceResult<()> {
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

    pub fn bind_memory<R: Into<BindResource>>(
        &self,
        resource: R,
        allocation: AllocationEntry,
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
    index: GenIndex<AllocatorInstance>,
}

#[derive(Debug, Clone, Copy)]
pub struct AllocationIndex {
    index: GuardIndex<Allocation>,
}

#[derive(Debug, Clone, Copy)]
pub struct AllocationEntry {
    allocator: AllocatorIndex,
    allocation: AllocationIndex,
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
    pub fn allocate(
        &mut self,
        context: &Context,
        index: AllocatorIndex,
        req: AllocReq,
    ) -> ResourceResult<AllocationEntry> {
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
    pub fn free(&mut self, context: &Context, index: AllocationEntry) -> ResourceResult<()> {
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
