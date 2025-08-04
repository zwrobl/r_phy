use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult, GenVec};

use crate::{
    memory::{
        allocator::{AllocationBorrow, Allocator, AllocatorIndex, AllocatorIndexTyped},
        error::{MemoryError, MemoryResult},
        AllocReqTyped, MemoryProperties,
    },
    Context,
};

use super::AllocationIndexTyped;

#[derive(Debug, Clone, Copy)]
pub struct PageConfig {}

impl Default for PageConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl PageConfig {
    #[inline]
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug)]
pub struct Page {}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}

impl Page {
    pub fn new() -> Self {
        Self {}
    }
}

impl Create for Page {
    type Config<'a> = PageConfig;
    type CreateError = MemoryError;

    fn create<'a, 'b>(
        _config: Self::Config<'a>,
        _context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        todo!()
    }
}

impl Destroy for Page {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> DestroyResult<Self> {
        todo!()
    }
}

impl Allocator for Page {
    type Storage = GenVec<Self>;

    #[inline]
    fn allocate<'a, M: MemoryProperties>(
        &mut self,
        _context: &Context,
        _req: AllocReqTyped<M>,
    ) -> MemoryResult<AllocationIndexTyped<M>> {
        todo!()
    }

    #[inline]
    fn free<'a, M: MemoryProperties>(
        &mut self,
        _context: &Context,
        _allocation: AllocationIndexTyped<M>,
    ) -> MemoryResult<()> {
        todo!()
    }

    #[inline]
    fn borrow<M: MemoryProperties>(
        &mut self,
        _allocation: AllocationIndexTyped<M>,
    ) -> MemoryResult<AllocationBorrow<M>> {
        todo!()
    }

    #[inline]
    fn put_back<'a, M: MemoryProperties>(
        &mut self,
        _allocation: super::AllocationBorrow<M>,
    ) -> MemoryResult<()> {
        todo!()
    }

    #[inline]
    fn wrap_index(index: AllocatorIndexTyped<Self>) -> AllocatorIndex {
        AllocatorIndex::Page(index)
    }
}
