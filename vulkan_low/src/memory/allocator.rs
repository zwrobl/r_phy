mod page;
mod r#static;
mod unpooled;

pub use page::*;
pub use r#static::*;
pub use unpooled::*;

use std::{
    cell::RefCell,
    collections::HashMap,
    convert::Infallible,
    ffi::c_void,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use ash::vk;
use type_kit::{
    BorrowedGuard, Create, Destroy, DestroyResult, DropGuard, FromGuard, GenCollection, GenIndex,
    GenIndexRaw, GenVec, GuardIndex, TypeGuard, TypeGuardVec,
};

use crate::{
    error::{AllocatorError, ResourceError, ResourceResult},
    memory::{
        range::ByteRange, AllocReq, AllocReqTyped, BindResource, DeviceLocal, HostCoherent,
        HostVisible, Memory, MemoryProperties, MemoryRaw, MemoryType,
    },
    Context,
};

type MemoryIndex<M> = GuardIndex<Memory<M>, TypeGuardVec<MemoryRaw>>;
type MemoryIndexRaw = GenIndexRaw;
#[derive(Debug, Clone, Copy)]
pub struct AllocationRaw {
    range: ByteRange,
    memory: MemoryIndexRaw,
}

#[derive(Debug, Clone, Copy)]
pub struct Allocation<M: MemoryProperties> {
    range: ByteRange,
    memory: MemoryIndex<M>,
}

pub struct AllocationBorrow<M: MemoryProperties> {
    range: ByteRange,
    memory: BorrowedGuard<Memory<M>, TypeGuardVec<MemoryRaw>>,
}

impl<M: MemoryProperties> Deref for AllocationBorrow<M> {
    type Target = Memory<M>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.memory
    }
}

impl<M: MemoryProperties> DerefMut for AllocationBorrow<M> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.memory
    }
}

impl<M: MemoryProperties> Allocation<M> {
    #[inline]
    pub fn new(memory: MemoryIndex<M>, range: ByteRange) -> Self {
        Self { range, memory }
    }

    /// # Safety
    /// This method allows user to create an Allocation instance of a specific memory type
    /// from an instance of Allocation of arbitrary memory type. This should be used only
    /// if it is known that the target memory type is indeed the same as the original one.
    pub unsafe fn cast<T: MemoryProperties>(self) -> Allocation<T> {
        Allocation {
            range: self.range,
            memory: MemoryIndex::<T>::from_inner(self.memory.into_inner()),
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
            memory: MemoryIndex::<M>::from_inner(inner.memory),
        }
    }
}

#[derive(Debug, Default)]
struct MemoryMap {
    usage: HashMap<TypeGuard<MemoryIndexRaw>, usize>,
    memory: TypeGuardVec<MemoryRaw>,
}

impl MemoryMap {
    #[inline]
    fn new() -> Self {
        Self {
            usage: HashMap::default(),
            memory: TypeGuardVec::default(),
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
    ) -> ResourceResult<Option<DropGuard<Memory<M>>>> {
        let memory = allocation.memory.into_guard();
        let count = self
            .usage
            .get_mut(&memory)
            .ok_or(AllocatorError::InvalidAllocationIndex)?;
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.usage.remove(&memory);
            let memory = self.memory.pop(allocation.memory)?;
            let memory = unsafe { Memory::<M>::from_inner(memory.into_inner()) };
            Ok(Some(DropGuard::new(memory)))
        } else {
            Ok(None)
        }
    }

    #[inline]
    fn borrow<M: MemoryProperties>(
        &mut self,
        allocation: Allocation<M>,
    ) -> ResourceResult<AllocationBorrow<M>> {
        let memory = self.memory.borrow(allocation.memory)?.try_into().unwrap();
        Ok(AllocationBorrow {
            range: allocation.range,
            memory,
        })
    }

    #[inline]
    fn put_back<M: MemoryProperties>(
        &mut self,
        allocation: AllocationBorrow<M>,
    ) -> ResourceResult<()> {
        let AllocationBorrow { memory, .. } = allocation;
        self.memory.put_back(memory.into())?;
        Ok(())
    }

    fn drain<M: MemoryProperties>(&mut self) -> Vec<MemoryIndex<M>> {
        let (valid, rest): (Vec<_>, Vec<_>) = self
            .usage
            .drain()
            .map(|(memory, count)| {
                MemoryIndex::<M>::try_from_guard(memory).map_err(|(memory, _)| (memory, count))
            })
            .partition(Result::is_ok);
        self.usage = rest.into_iter().map(Result::unwrap_err).collect();
        valid.into_iter().map(Result::unwrap).collect()
    }

    #[inline]
    fn free_memory_type<M: MemoryProperties>(&mut self, context: &Context) {
        let memory_indices = self.drain::<M>();
        memory_indices.into_iter().for_each(|index| {
            let mut memory = self.memory.pop(index).unwrap();
            let _ = memory.destroy(context);
        });
    }

    #[inline]
    fn free_memory(&mut self, context: &Context) {
        self.free_memory_type::<DeviceLocal>(context);
        self.free_memory_type::<HostCoherent>(context);
        self.free_memory_type::<HostVisible>(context);
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
    allocations: TypeGuardVec<AllocationRaw>,
    memory_map: MemoryMap,
}

impl Default for AllocationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AllocationStore {
    #[inline]
    pub fn new() -> Self {
        Self {
            allocations: TypeGuardVec::default(),
            memory_map: MemoryMap::new(),
        }
    }

    #[inline]
    pub fn allocate<M: MemoryProperties>(
        &mut self,
        context: &Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<MemoryIndex<M>> {
        let alloc_info = context.get_memory_allocate_info(req)?;
        let memory = Memory::<M>::create(alloc_info, context)?;
        let index = self.memory_map.memory.push(memory.into_guard())?;
        Ok(index)
    }

    #[inline]
    pub fn suballocate<M: MemoryProperties>(
        &mut self,
        req: AllocReqTyped<M>,
        memory: MemoryIndex<M>,
    ) -> ResourceResult<AllocationIndex<M>> {
        let req = req.requirements();
        let mut borrow: BorrowedGuard<Memory<M>, _> =
            self.memory_map.memory.borrow(memory)?.try_into().unwrap();
        let alloc = borrow
            .suballocate(req.size as usize, req.alignment as usize)
            .and_then(|range| Some(Allocation { range, memory }))
            .ok_or(ResourceError::AllocatorError(AllocatorError::OutOfMemory));
        self.memory_map.memory.put_back(borrow.into())?;
        alloc.and_then(|allocation| self.register_allocation(allocation))
    }

    #[inline]
    pub fn register_allocation<M: MemoryProperties>(
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
    ) -> ResourceResult<Option<DropGuard<Memory<M>>>> {
        let allocation = self.allocations.pop(index.index)?;
        let allocation = unsafe { Allocation::<M>::from_inner(allocation.into_inner()) };
        self.memory_map.pop(allocation)
    }

    #[inline]
    pub fn borrow<M: MemoryProperties>(
        &mut self,
        index: AllocationIndex<M>,
    ) -> ResourceResult<AllocationBorrow<M>> {
        let allocation =
            Allocation::<M>::try_from_guard(*self.allocations.get(index.index)?).unwrap();
        self.memory_map.borrow(allocation)
    }

    #[inline]
    pub fn put_back<M: MemoryProperties>(
        &mut self,
        allocation: AllocationBorrow<M>,
    ) -> ResourceResult<()> {
        self.memory_map.put_back(allocation)
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
    fn allocate<M: MemoryProperties>(
        &mut self,
        context: &Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationIndex<M>>;

    fn free<M: MemoryProperties>(
        &mut self,
        context: &Context,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<()>;

    fn borrow<'a, M: MemoryProperties>(
        &mut self,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<AllocationBorrow<M>>;

    fn put_back<'a, M: MemoryProperties>(
        &mut self,
        allocation: AllocationBorrow<M>,
    ) -> ResourceResult<()>;
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
    pub fn borrow<M: MemoryProperties>(
        &mut self,
        index: AllocationIndex<M>,
    ) -> ResourceResult<AllocationBorrow<M>> {
        match self {
            AllocatorInstance::Page(allocator) => allocator.borrow(index),
            AllocatorInstance::Unpooled(allocator) => allocator.borrow(index),
            AllocatorInstance::Static(allocator) => allocator.borrow(index),
        }
    }

    #[inline]
    pub fn put_back<M: MemoryProperties>(
        &mut self,
        allocation: AllocationBorrow<M>,
    ) -> ResourceResult<()> {
        match self {
            AllocatorInstance::Page(allocator) => allocator.put_back(allocation),
            AllocatorInstance::Unpooled(allocator) => allocator.put_back(allocation),
            AllocatorInstance::Static(allocator) => allocator.put_back(allocation),
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
        let ptr = self.operate_alloc(allocation, |allocation| {
            let range = allocation.range;
            // TODO: Improve error handling type hierary to avoid unsafe unwrap
            unsafe {
                allocation
                    .map(self)
                    .unwrap()
                    .byte_offset(range.beg as isize)
            }
        })?;
        Ok(ptr)
    }

    pub fn unmap_allocation<M: MemoryProperties>(
        &self,
        allocation: AllocationEntry<M>,
    ) -> ResourceResult<()> {
        self.operate_alloc(allocation, |allocation| allocation.unmap(self))
    }

    pub fn bind_memory<R: Into<BindResource>, M: MemoryProperties>(
        &self,
        resource: R,
        allocation: AllocationEntry<M>,
    ) -> ResourceResult<()> {
        self.operate_alloc(allocation, |allocation| {
            let range = allocation.range;
            match resource.into() {
                BindResource::Image(image) => unsafe {
                    self.bind_image_memory(image, ***allocation, range.beg as vk::DeviceSize)
                },
                BindResource::Buffer(buffer) => unsafe {
                    self.bind_buffer_memory(buffer, ***allocation, range.beg as vk::DeviceSize)
                },
            }
        })??;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AllocatorIndex {
    index: GenIndex<AllocatorInstance, GenVec<AllocatorInstance>>,
}

#[derive(Debug)]
pub struct AllocationIndex<M: MemoryProperties> {
    index: GuardIndex<Allocation<M>, TypeGuardVec<AllocationRaw>>,
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
    allocators: RefCell<GenVec<AllocatorInstance>>,
}

impl Default for AllocatorStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl AllocatorStorage {
    #[inline]
    pub fn new() -> Self {
        Self {
            allocators: RefCell::new(GenVec::new()),
        }
    }

    #[inline]
    pub fn create_allocator<'a, 'b, A: Allocator>(
        &self,
        context: &'a Context,
        config: A::Config<'b>,
    ) -> ResourceResult<AllocatorIndex> {
        let allocator = A::create(config, context)?.into();
        let index = self.allocators.borrow_mut().push(allocator)?;
        Ok(AllocatorIndex { index })
    }

    #[inline]
    pub fn destroy_allocator(
        &self,
        context: &Context,
        index: AllocatorIndex,
    ) -> ResourceResult<()> {
        let _ = self
            .allocators
            .borrow_mut()
            .pop(index.index)?
            .destroy(context);
        Ok(())
    }

    #[inline]
    pub fn allocate<M: MemoryProperties>(
        &self,
        context: &Context,
        index: AllocatorIndex,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationEntry<M>> {
        let allocation = self
            .allocators
            .borrow_mut()
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
        &self,
        context: &Context,
        index: AllocationEntry<M>,
    ) -> ResourceResult<()> {
        let AllocationEntry {
            allocator,
            allocation,
        } = index;
        self.allocators
            .borrow_mut()
            .get_mut(allocator.index)?
            .free(context, allocation)
    }

    #[inline]
    pub fn operate_mut<M: MemoryProperties, R, F: FnOnce(&mut AllocationBorrow<M>) -> R>(
        &self,
        index: AllocationEntry<M>,
        f: F,
    ) -> ResourceResult<R> {
        let AllocationEntry {
            allocator,
            allocation,
        } = index;
        let mut allocation = self
            .allocators
            .borrow_mut()
            .get_mut(allocator.index)?
            .borrow(allocation)?;
        let ret = f(&mut allocation);
        self.allocators
            .borrow_mut()
            .get_mut(allocator.index)?
            .put_back(allocation)?;
        Ok(ret)
    }

    #[inline]
    pub fn destroy_storage(&self, context: &Context) -> ResourceResult<()> {
        let _ = self.allocators.borrow_mut().destroy(context);
        Ok(())
    }
}
