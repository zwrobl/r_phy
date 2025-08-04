mod map;
mod page;
mod r#static;
mod storage;
mod unpooled;

pub use map::*;
pub use page::*;
pub use r#static::*;
pub use storage::*;
pub use unpooled::*;

use std::{
    convert::Infallible,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use type_kit::{
    BorrowedGuard, Create, Destroy, FromGuard, GenCollection, GenIndexRaw, GuardIndex, GuardVec,
};

use crate::{
    memory::{
        error::{MemoryError, MemoryResult},
        range::ByteRange,
        AllocReq, AllocReqTyped, DeviceLocal, HostCoherent, HostVisible, Memory, MemoryProperties,
        MemoryRaw,
    },
    Context,
};

pub type MemoryIndex<M> = GuardIndex<Memory<M>, GuardVec<MemoryRaw>>;
pub type MemoryIndexRaw = GenIndexRaw;
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
    memory: BorrowedGuard<Memory<M>, GuardVec<MemoryRaw>>,
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

pub trait AllocatorBuilder {
    fn with_allocation<M: MemoryProperties>(&mut self, req: AllocReqTyped<M>) -> &mut Self;
}

pub trait Allocator: 'static + Sized
where
    for<'a> Self: Destroy<Context<'a> = &'a Context, DestroyError = Infallible>
        + Create<CreateError = MemoryError>,
{
    type Storage: GenCollection<Self>
        + for<'a> Destroy<Context<'a> = &'a Context, DestroyError = Infallible>;

    fn allocate<M: MemoryProperties>(
        &mut self,
        context: &Context,
        req: AllocReqTyped<M>,
    ) -> MemoryResult<AllocationIndexTyped<M>>;

    fn free<M: MemoryProperties>(
        &mut self,
        context: &Context,
        allocation: AllocationIndexTyped<M>,
    ) -> MemoryResult<()>;

    fn borrow<'a, M: MemoryProperties>(
        &mut self,
        allocation: AllocationIndexTyped<M>,
    ) -> MemoryResult<AllocationBorrow<M>>;

    fn put_back<'a, M: MemoryProperties>(
        &mut self,
        allocation: AllocationBorrow<M>,
    ) -> MemoryResult<()>;

    fn wrap_index(index: AllocatorIndexTyped<Self>) -> AllocatorIndex;
}

#[derive(Debug)]
pub struct AllocationIndexTyped<M: MemoryProperties> {
    index: GuardIndex<Allocation<M>, GuardVec<AllocationRaw>>,
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
