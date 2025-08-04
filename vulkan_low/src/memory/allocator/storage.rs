use std::{cell::RefCell, ffi::c_void, fmt::Debug};

use ash::vk;
use type_kit::{
    list_type, Cons, Contains, Destroy, FromGuard, GenCell, GenCollection, GenIndex, GenIndexRaw,
    GenVec, Marker, Nil,
};

use crate::{
    error::ResourceResult,
    memory::{
        allocator::{
            AllocationBorrow, AllocationIndex, AllocationIndexTyped, Allocator, Page, Static,
            Unpooled,
        },
        AllocReqTyped, BindResource, DeviceLocal, HostCoherent, HostVisible, MemoryProperties,
    },
    Context,
};

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

    #[inline]
    pub fn unmap_allocation<M: MemoryProperties>(
        &self,
        allocation: AllocationEntryTyped<M>,
    ) -> ResourceResult<()> {
        self.operate_alloc(allocation, |allocation| allocation.unmap(self))
    }

    #[inline]
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
