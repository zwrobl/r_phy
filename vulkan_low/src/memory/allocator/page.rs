use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult};

use crate::{
    error::{ResourceError, ResourceResult},
    memory::{
        allocator::{AllocationBorrow, Allocator, AllocatorInstance},
        AllocReqTyped, MemoryProperties,
    },
    Context,
};

use super::AllocationIndex;

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
    type CreateError = ResourceError;

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

impl From<Page> for AllocatorInstance {
    #[inline]
    fn from(value: Page) -> Self {
        AllocatorInstance::Page(value)
    }
}

impl Allocator for Page {
    #[inline]
    fn allocate<'a, M: MemoryProperties>(
        &mut self,
        _context: &Context,
        _req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationIndex<M>> {
        todo!()
    }

    #[inline]
    fn free<'a, M: MemoryProperties>(
        &mut self,
        _context: &Context,
        _allocation: AllocationIndex<M>,
    ) -> ResourceResult<()> {
        todo!()
    }

    fn borrow<M: MemoryProperties>(
        &mut self,
        _allocation: AllocationIndex<M>,
    ) -> ResourceResult<AllocationBorrow<M>> {
        todo!()
    }

    fn put_back<'a, M: MemoryProperties>(
        &mut self,
        _allocation: super::AllocationBorrow<M>,
    ) -> ResourceResult<()> {
        todo!()
    }
}
