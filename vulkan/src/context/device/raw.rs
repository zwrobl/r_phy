use type_kit::{Cons, Destroy, TypedNil};

use crate::context::{device::raw::allocator::AllocatorBuilder, Context};

pub mod allocator;
pub mod range;
pub mod resources;

pub trait Partial
where
    for<'a> Self: Destroy<Context<'a> = &'a Context>,
{
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B);
}

impl<T: Partial> Partial for Vec<T> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.iter()
            .for_each(|item| item.register_memory_requirements(builder));
    }
}

impl<T: Partial> Partial for Option<T> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        if let Some(value) = self.as_ref() {
            value.register_memory_requirements(builder);
        }
    }
}

impl<T: Partial, N: Partial> Partial for Cons<T, N> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.head.register_memory_requirements(builder);
        self.tail.register_memory_requirements(builder);
    }
}

impl<T: Partial> Partial for TypedNil<T> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, _builder: &mut B) {}
}
