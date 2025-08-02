use type_kit::{Cons, Destroy, DropGuard, TypedNil};

use crate::{device::raw::allocator::AllocatorBuilder, Context};

pub mod allocator;
pub mod range;
pub mod resources;

pub trait Partial
where
    for<'a> Self: Destroy<Context<'a> = &'a Context>,
{
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B);
}

// For now, we manually wrap partial instance in the DropGuard,
// and all implementations of Create using Partial instances as Config,
// require them to be wrapped in DropGuard, this is so external user can
// prepare resource and query for its memory requirements wihtout need to
// insert it to the Context ResourceStorage, yet enforcing that the resource is properly destroyed
// either when target Resource is created or if not the partial .destroy() is called manually.
// TODO: Come up with a better way of enforcing this pattern
impl<T: Partial> Partial for DropGuard<T> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, _builder: &mut B) {
        self.as_ref().register_memory_requirements(_builder);
    }
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
