use type_kit::Destroy;

use crate::context::{device::raw::allocator::AllocatorBuilder, error::VkResult, Context};

pub mod allocator;
pub mod range;
pub mod resources;

pub trait Partial
where
    for<'a> Self: Destroy<Context<'a> = &'a Context>,
{
    fn prepare(config: Self::Config, context: &Context) -> VkResult<Self>;  
    fn requirements<B: AllocatorBuilder>(&self, builder: &mut B);
}
