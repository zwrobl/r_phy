use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult};

use crate::context::{
    device::{
        memory::{AllocReqTyped, MemoryProperties},
        raw::allocator::{Allocation, Allocator, AllocatorInstance},
    },
    error::{ResourceError, ResourceResult},
    Context,
};

use super::AllocationIndex;

#[derive(Debug, Clone, Copy)]
pub struct PageConfig {}

impl PageConfig {
    #[inline]
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug)]
pub struct Page {}

impl Page {
    pub fn new() -> Self {
        Self {}
    }
}

impl Create for Page {
    type Config<'a> = PageConfig;
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        todo!()
    }
}

impl Destroy for Page {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
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
        context: &crate::Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationIndex<M>> {
        todo!()
    }

    #[inline]
    fn free<'a, M: MemoryProperties>(
        &mut self,
        context: &crate::Context,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<()> {
        todo!()
    }

    fn get_allocation<M: MemoryProperties>(
        &self,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<Allocation<M>> {
        todo!()
    }
}
