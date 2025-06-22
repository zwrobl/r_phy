use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult, FromGuard, GenCollection, Valid};

use crate::context::{
    device::{
        raw::{
            allocator::{AllocationRequest, Allocator, AllocatorIndex},
            resources::{memory::Memory, ResourceIndex},
        },
        resources::buffer::ByteRange,
    },
    error::{AllocatorError, ResourceResult},
    Context,
};

use super::{AllocationIndex, AllocatorState, State, Strategy};

pub struct LinearBuffer {
    memory: ResourceIndex<Memory>,
    range: ByteRange,
}

impl From<Valid<LinearBuffer>> for LinearBuffer {
    #[inline]
    fn from(value: Valid<LinearBuffer>) -> Self {
        let buffer = value.into_inner();
        buffer
    }
}

impl FromGuard for LinearBuffer {
    type Inner = LinearBuffer;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self
    }
}

pub struct LinearConfig {}

impl From<LinearConfig> for LinearState {
    #[inline]
    fn from(_: LinearConfig) -> Self {
        LinearState {}
    }
}

#[derive(Debug)]
pub struct LinearState {}

impl From<LinearState> for AllocatorState {
    #[inline]
    fn from(state: LinearState) -> Self {
        AllocatorState::Linear(state)
    }
}

impl State for LinearState {
    #[inline]
    fn try_get(state: &AllocatorState) -> Result<&Self, AllocatorError> {
        match state {
            AllocatorState::Linear(config) => Ok(config),
            _ => Err(AllocatorError::InvalidConfiguration),
        }
    }
}

pub struct Linear {
    buffers: GenCollection<LinearBuffer>,
}

impl Linear {
    #[inline]
    pub fn new() -> Self {
        Self {
            buffers: GenCollection::default(),
        }
    }
}

impl Create for Linear {
    type Config<'a> = ();
    type CreateError = AllocatorError;

    #[inline]
    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        todo!()
    }
}

impl Destroy for Linear {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        todo!()
    }
}

impl Strategy for Linear {
    type State = LinearState;
    type CreateConfig<'a> = LinearConfig;

    #[inline]
    fn wrap_index(index: type_kit::GuardIndex<Allocator<Self>>) -> AllocatorIndex {
        AllocatorIndex::Linear(index)
    }

    fn allocate<'a>(
        allocator: type_kit::ScopedInnerMut<'a, Allocator<Self>>,
        context: &crate::Context,
        req: AllocationRequest,
    ) -> ResourceResult<AllocationIndex> {
        todo!()
    }

    fn free<'a>(
        allocator: type_kit::ScopedInnerMut<'a, Allocator<Self>>,
        context: &crate::Context,
        allocation: AllocationIndex,
    ) -> ResourceResult<()> {
        todo!()
    }
}
