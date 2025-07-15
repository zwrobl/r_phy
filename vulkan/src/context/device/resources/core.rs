use std::convert::Infallible;

use type_kit::{Create, CreateResult, Destroy, DestroyResult};

use crate::context::{
    device::{memory::AllocReq, raw::Partial},
    error::VkResult,
    Context,
};

pub mod image;

pub trait PartialBuilder<'a>: Sized {
    type Config;
    type Target;

    fn prepare(config: Self::Config, context: &Context) -> VkResult<Self>;
    fn requirements(&self) -> impl Iterator<Item = AllocReq>;
}

pub struct DummyPack {}

impl Create for DummyPack {
    type Config<'a> = ();
    type CreateError = Infallible;

    fn create<'a, 'b>(_: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
        unreachable!()
    }
}

impl Destroy for DummyPack {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, _: Self::Context<'a>) -> DestroyResult<Self> {
        unreachable!()
    }
}

impl Partial for DummyPack {
    fn register_memory_requirements<B: crate::context::device::raw::allocator::AllocatorBuilder>(
        &self,
        _builder: &mut B,
    ) {
        unreachable!()
    }
}
