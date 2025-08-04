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
    list_type, BorrowedGuard, Cons, Contains, Create, Destroy, DestroyResult, DropGuard, FromGuard,
    GenCell, GenCollection, GenIndex, GenIndexRaw, GenVec, GuardIndex, Marker, Nil, TypeGuard,
    TypeGuardVec,
};

use crate::{
    error::{AllocatorError, ResourceError, ResourceResult},
    memory::{
        range::ByteRange, AllocReq, AllocReqTyped, BindResource, DeviceLocal, HostCoherent,
        HostVisible, Memory, MemoryProperties, MemoryRaw,
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
    ) -> ResourceResult<AllocationIndexTyped<M>> {
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
    ) -> ResourceResult<AllocationIndexTyped<M>> {
        self.memory_map.register(&allocation);
        let index = self.allocations.push(allocation.into_guard())?;
        Ok(AllocationIndexTyped { index })
    }

    #[inline]
    pub fn pop<M: MemoryProperties>(
        &mut self,
        index: AllocationIndexTyped<M>,
    ) -> ResourceResult<Option<DropGuard<Memory<M>>>> {
        let allocation = self.allocations.pop(index.index)?;
        let allocation = unsafe { Allocation::<M>::from_inner(allocation.into_inner()) };
        self.memory_map.pop(allocation)
    }

    #[inline]
    pub fn borrow<M: MemoryProperties>(
        &mut self,
        index: AllocationIndexTyped<M>,
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
        + Create<CreateError = ResourceError>,
{
    type Storage: GenCollection<Self>
        + for<'a> Destroy<Context<'a> = &'a Context, DestroyError = Infallible>;

    fn allocate<M: MemoryProperties>(
        &mut self,
        context: &Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationIndexTyped<M>>;

    fn free<M: MemoryProperties>(
        &mut self,
        context: &Context,
        allocation: AllocationIndexTyped<M>,
    ) -> ResourceResult<()>;

    fn borrow<'a, M: MemoryProperties>(
        &mut self,
        allocation: AllocationIndexTyped<M>,
    ) -> ResourceResult<AllocationBorrow<M>>;

    fn put_back<'a, M: MemoryProperties>(
        &mut self,
        allocation: AllocationBorrow<M>,
    ) -> ResourceResult<()>;

    fn wrap_index(index: AllocatorIndexTyped<Self>) -> AllocatorIndex;
}

impl Context {
    pub fn map_allocation<M: MemoryProperties>(
        &self,
        allocation: AllocationEntryTyped<M>,
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
        allocation: AllocationEntryTyped<M>,
    ) -> ResourceResult<()> {
        self.operate_alloc(allocation, |allocation| allocation.unmap(self))
    }

    pub fn bind_memory<R: Into<BindResource>, M: MemoryProperties>(
        &self,
        resource: R,
        allocation: AllocationEntryTyped<M>,
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

#[derive(Debug)]
pub struct AllocatorIndexTyped<A: Allocator> {
    index: GenIndex<A, A::Storage>,
}

impl<A: Allocator> From<AllocatorIndexTyped<A>> for Option<AllocatorIndex> {
    #[inline]
    fn from(value: AllocatorIndexTyped<A>) -> Self {
        Some(A::wrap_index(value))
    }
}

impl<A: Allocator> From<AllocatorIndexTyped<A>> for AllocatorIndex {
    #[inline]
    fn from(value: AllocatorIndexTyped<A>) -> Self {
        A::wrap_index(value)
    }
}

impl<A: Allocator> Clone for AllocatorIndexTyped<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: Allocator> Copy for AllocatorIndexTyped<A> {}

impl<A: Allocator> From<GenIndex<A, A::Storage>> for AllocatorIndexTyped<A> {
    #[inline]
    fn from(index: GenIndex<A, A::Storage>) -> Self {
        Self { index }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AllocatorIndexRaw {
    index: GenIndexRaw,
}

impl<A: Allocator> FromGuard for AllocatorIndexTyped<A> {
    type Inner = AllocatorIndexRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        AllocatorIndexRaw {
            index: self.index.into_inner(),
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            index: GenIndex::from_inner(inner.index),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AllocatorIndex {
    Static(AllocatorIndexTyped<Static>),
    Page(AllocatorIndexTyped<Page>),
    Unpooled(AllocatorIndexTyped<Unpooled>),
}

#[derive(Debug)]
pub struct AllocationIndexTyped<M: MemoryProperties> {
    index: GuardIndex<Allocation<M>, TypeGuardVec<AllocationRaw>>,
}

impl<M: MemoryProperties> Clone for AllocationIndexTyped<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: MemoryProperties> Copy for AllocationIndexTyped<M> {}

#[derive(Debug, Clone, Copy)]
pub enum AllocationIndex {
    DeviceLocal(AllocationIndexTyped<DeviceLocal>),
    HostCoherent(AllocationIndexTyped<HostCoherent>),
    HostVisible(AllocationIndexTyped<HostVisible>),
}

impl AllocationIndex {
    #[inline]
    fn into_inner(&self) -> AllocationIndexRaw {
        match self {
            Self::DeviceLocal(index) => index.into_inner(),
            Self::HostCoherent(index) => index.into_inner(),
            Self::HostVisible(index) => index.into_inner(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AllocationIndexRaw {
    index: GenIndexRaw,
}

impl<M: MemoryProperties> FromGuard for AllocationIndexTyped<M> {
    type Inner = AllocationIndexRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        AllocationIndexRaw {
            index: self.index.into_inner(),
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            index: GuardIndex::<Allocation<M>, _>::from_inner(inner.index),
        }
    }
}

#[derive(Debug)]
pub struct AllocationEntryTyped<M: MemoryProperties> {
    allocator: AllocatorIndex,
    allocation: AllocationIndexTyped<M>,
}

impl<M: MemoryProperties> Clone for AllocationEntryTyped<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: MemoryProperties> Copy for AllocationEntryTyped<M> {}

#[derive(Debug, Clone, Copy)]
pub struct AllocationEntry {
    allocator: AllocatorIndex,
    allocation: AllocationIndex,
}

impl<M: MemoryProperties> FromGuard for AllocationEntryTyped<M> {
    type Inner = AllocationEntry;

    fn into_inner(self) -> Self::Inner {
        AllocationEntry {
            allocator: self.allocator,
            allocation: M::wrap_index(self.allocation),
        }
    }

    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            allocator: inner.allocator,
            allocation: AllocationIndexTyped::<M>::from_inner(inner.allocation.into_inner()),
        }
    }
}

impl Context {
    #[inline]
    pub(crate) fn free_allocation_raw(&self, allocation: AllocationEntry) -> ResourceResult<()> {
        let AllocationEntry {
            allocator,
            allocation,
        } = allocation;
        match allocation {
            AllocationIndex::DeviceLocal(allocation) => {
                self.free::<DeviceLocal>(AllocationEntryTyped {
                    allocator,
                    allocation,
                })
            }
            AllocationIndex::HostCoherent(allocation) => {
                self.free::<HostCoherent>(AllocationEntryTyped {
                    allocator,
                    allocation,
                })
            }
            AllocationIndex::HostVisible(allocation) => {
                self.free::<HostVisible>(AllocationEntryTyped {
                    allocator,
                    allocation,
                })
            }
        }
    }
}

pub type AllocatorStorageList = list_type![GenVec<Static>, GenVec<Page>, GenCell<Unpooled>, Nil];

pub struct AllocatorStorage {
    allocators: RefCell<AllocatorStorageList>,
    pub default_allocator: AllocatorIndexTyped<Unpooled>,
}

impl Default for AllocatorStorage {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl AllocatorStorage {
    #[inline]
    pub fn new() -> Self {
        let mut allocators = AllocatorStorageList::default();
        let index = allocators
            .get_mut::<<Unpooled as Allocator>::Storage, _>()
            .push(Unpooled::default())
            .unwrap()
            .into();
        Self {
            allocators: RefCell::new(allocators),
            default_allocator: index,
        }
    }

    #[inline]
    pub fn push_allocator<'a, 'b, A: Allocator<Storage = GenVec<A>>, M: Marker>(
        &self,
        allocator: A,
    ) -> ResourceResult<AllocatorIndexTyped<A>>
    where
        AllocatorStorageList: Contains<A::Storage, M>,
    {
        let index = self
            .allocators
            .borrow_mut()
            .get_mut()
            .push(allocator)?
            .into();
        Ok(index)
    }

    #[inline]
    pub fn pop_allocator<A: Allocator<Storage = GenVec<A>>, M: Marker>(
        &self,
        index: AllocatorIndexTyped<A>,
    ) -> ResourceResult<A>
    where
        AllocatorStorageList: Contains<A::Storage, M>,
    {
        let allocator = self.allocators.borrow_mut().get_mut().pop(index.index)?;
        Ok(allocator)
    }

    #[inline]
    pub fn allocate<M: MemoryProperties>(
        &self,
        context: &Context,
        req: AllocReqTyped<M>,
        allocator: Option<AllocatorIndex>,
    ) -> ResourceResult<AllocationEntryTyped<M>> {
        let allocator = allocator.unwrap_or(self.default_allocator.into());
        match allocator {
            AllocatorIndex::Static(index) => {
                self.allocate_impl::<M, Static, _>(context, index, req)
            }
            AllocatorIndex::Page(index) => self.allocate_impl::<M, Page, _>(context, index, req),
            AllocatorIndex::Unpooled(index) => {
                self.allocate_impl::<M, Unpooled, _>(context, index, req)
            }
        }
    }

    #[inline]
    fn allocate_impl<M: MemoryProperties, A: Allocator, T: Marker>(
        &self,
        context: &Context,
        index: AllocatorIndexTyped<A>,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationEntryTyped<M>>
    where
        AllocatorStorageList: Contains<A::Storage, T>,
    {
        let allocation = self
            .allocators
            .borrow_mut()
            .get_mut()
            .get_mut(index.index)?
            .allocate(context, req)?;
        let entry = AllocationEntryTyped {
            allocator: A::wrap_index(index),
            allocation,
        };
        Ok(entry)
    }

    #[inline]
    pub fn free<M: MemoryProperties>(
        &self,
        context: &Context,
        index: AllocationEntryTyped<M>,
    ) -> ResourceResult<()> {
        let AllocationEntryTyped {
            allocator,
            allocation,
        } = index;
        match allocator {
            AllocatorIndex::Static(allocator) => {
                self.free_impl::<M, Static, _>(context, allocator, allocation)
            }
            AllocatorIndex::Page(allocator) => {
                self.free_impl::<M, Page, _>(context, allocator, allocation)
            }
            AllocatorIndex::Unpooled(allocator) => {
                self.free_impl::<M, Unpooled, _>(context, allocator, allocation)
            }
        }
    }

    #[inline]
    fn free_impl<M: MemoryProperties, A: Allocator, T: Marker>(
        &self,
        context: &Context,
        allocator_index: AllocatorIndexTyped<A>,
        allocation_index: AllocationIndexTyped<M>,
    ) -> ResourceResult<()>
    where
        AllocatorStorageList: Contains<A::Storage, T>,
    {
        self.allocators
            .borrow_mut()
            .get_mut()
            .get_mut(allocator_index.index)?
            .free(context, allocation_index)
    }

    #[inline]
    pub fn operate_mut<M: MemoryProperties, R, F: FnOnce(&mut AllocationBorrow<M>) -> R>(
        &self,
        index: AllocationEntryTyped<M>,
        f: F,
    ) -> ResourceResult<R> {
        let AllocationEntryTyped {
            allocator,
            allocation,
        } = index;
        match allocator {
            AllocatorIndex::Static(allocator) => {
                self.operate_mut_impl::<M, Static, _, _, _>(allocator, allocation, f)
            }
            AllocatorIndex::Page(allocator) => {
                self.operate_mut_impl::<M, Page, _, _, _>(allocator, allocation, f)
            }
            AllocatorIndex::Unpooled(allocator) => {
                self.operate_mut_impl::<M, Unpooled, _, _, _>(allocator, allocation, f)
            }
        }
    }

    #[inline]
    fn operate_mut_impl<
        M: MemoryProperties,
        A: Allocator,
        T: Marker,
        R,
        F: FnOnce(&mut AllocationBorrow<M>) -> R,
    >(
        &self,
        allocator_index: AllocatorIndexTyped<A>,
        allocation_index: AllocationIndexTyped<M>,
        f: F,
    ) -> ResourceResult<R>
    where
        AllocatorStorageList: Contains<A::Storage, T>,
    {
        let mut allocation = self
            .allocators
            .borrow_mut()
            .get_mut()
            .get_mut(allocator_index.index)?
            .borrow(allocation_index)?;
        let ret = f(&mut allocation);
        self.allocators
            .borrow_mut()
            .get_mut()
            .get_mut(allocator_index.index)?
            .put_back(allocation)?;
        Ok(ret)
    }

    #[inline]
    pub fn destroy_storage(&self, context: &Context) -> ResourceResult<()> {
        let _ = self.allocators.borrow_mut().destroy(context);
        Ok(())
    }
}
