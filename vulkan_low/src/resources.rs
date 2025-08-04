pub mod buffer;
pub mod command;
pub mod descriptor;
pub mod framebuffer;
pub mod image;
pub mod layout;
pub mod pipeline;
pub mod render_pass;
pub mod storage;
pub mod swapchain;

use std::fmt::Debug;

use type_kit::{
    Cons, Contains, Create, Destroy, DropGuard, FromGuard, GenIndexRaw, GuardCollectionT,
    GuardIndex, Marked, Marker, TypedNil,
};

use crate::{error::ResourceError, memory::allocator::AllocatorBuilder, Context};

pub trait Resource:
    FromGuard<Inner = Self::RawType>
    + for<'a> Create<Context<'a> = &'a Context, CreateError = ResourceError>
{
    type RawType: Clone + Copy + for<'a> Destroy<Context<'a> = Self::Context<'a>>;
    type RawCollection: GuardCollectionT<Self::RawType>;
}

pub type Raw<R> = <R as Resource>::RawType;

pub struct ResourceIndex<R: Resource> {
    index: GuardIndex<R, R::RawCollection>,
}

impl<R: Resource> ResourceIndex<R> {
    #[inline]
    pub fn unwrap(self) -> GuardIndex<R, R::RawCollection> {
        self.index
    }

    #[inline]
    pub fn mark<L, M: Marker>(self) -> Marked<Self, M>
    where
        L: Contains<R::RawCollection, M>,
    {
        Marked::new(self)
    }
}

pub type RawIndex = GenIndexRaw;

impl<R: Resource> Clone for ResourceIndex<R> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Resource> Copy for ResourceIndex<R> {}

impl<R: Resource> Debug for ResourceIndex<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceIndex")
            .field("index", &self.index)
            .finish()
    }
}

impl<R: Resource> FromGuard for ResourceIndex<R> {
    type Inner = GenIndexRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self.index.into_inner()
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            index: GuardIndex::<R, R::RawCollection>::from_inner(inner),
        }
    }
}

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
