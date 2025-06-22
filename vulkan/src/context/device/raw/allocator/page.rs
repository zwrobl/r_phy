use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult};

use crate::context::{
    device::{
        memory::MemoryProperties,
        raw::allocator::{AllocationRequest, Allocator, AllocatorIndex},
    },
    error::{AllocatorError, ResourceResult},
    Context,
};

use super::{AllocationIndex, AllocatorState, State, Strategy};

#[derive(Debug, Clone, Copy)]
pub struct PageConfig {
    page_size: u64,
}

impl PageConfig {
    #[inline]
    pub fn new(page_size: u64) -> Self {
        Self { page_size }
    }
}

impl From<PageConfig> for PageState {
    #[inline]
    fn from(value: PageConfig) -> Self {
        Self {
            page_size: value.page_size,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PageState {
    page_size: u64,
}

impl From<PageState> for AllocatorState {
    #[inline]
    fn from(config: PageState) -> Self {
        AllocatorState::Page(config)
    }
}

impl State for PageState {
    #[inline]
    fn try_get(state: &AllocatorState) -> Result<&Self, AllocatorError> {
        match state {
            AllocatorState::Page(config) => Ok(config),
            _ => Err(AllocatorError::InvalidConfiguration),
        }
    }
}

pub struct Page {}

impl Page {
    pub fn new() -> Self {
        Self {}
    }
}

impl Create for Page {
    type Config<'a> = ();
    type CreateError = AllocatorError;

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

impl Strategy for Page {
    type State = PageState;
    type CreateConfig<'a> = PageConfig;

    #[inline]
    fn wrap_index(index: type_kit::GuardIndex<Allocator<Self>>) -> AllocatorIndex {
        AllocatorIndex::Page(index)
    }

    #[inline]
    fn allocate<'a>(
        allocator: type_kit::ScopedInnerMut<'a, Allocator<Self>>,
        context: &crate::Context,
        req: AllocationRequest,
    ) -> ResourceResult<AllocationIndex> {
        todo!()
    }

    #[inline]
    fn free<'a>(
        allocator: type_kit::ScopedInnerMut<'a, Allocator<Self>>,
        context: &crate::Context,
        allocation: AllocationIndex,
    ) -> ResourceResult<()> {
        todo!()
    }
}
