mod material;
mod mesh;
mod skybox;

pub use material::*;
pub use mesh::*;
pub use skybox::*;

use std::convert::Infallible;

use type_kit::{Create, CreateResult, Destroy, DestroyResult};

use vulkan_low::{
    device::raw::{allocator::AllocatorBuilder, Partial},
    Context,
};

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
    fn register_memory_requirements<B: AllocatorBuilder>(&self, _builder: &mut B) {
        unreachable!()
    }
}
