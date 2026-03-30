use std::{
    any::type_name,
    fmt::{Debug, Formatter},
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{Cons, Destroy, DestroyResult};

/// Marker trait for type list type position identification.
pub trait Marker: 'static + Clone + Copy + Send + Sync {}

/// Appends index value to the simple `Marker` types in which
/// linear position in the type list is known at compile time.
/// e.g. `Here`, There<Here>, There<There<Here>>, etc.
pub trait IndexedMarker: Marker {
    const INDEX: usize;
}

/// Marker denoting the target position in the type list
/// Terminates the marker chain.
#[derive(Debug, Default, Clone, Copy)]
pub struct Here {}

impl Marker for Here {}

impl IndexedMarker for Here {
    const INDEX: usize = 0;
}

/// Marker denoting a position further in the type list.
pub struct There<T: Marker> {
    _phantom: PhantomData<T>,
}

impl<T: Marker> Debug for There<T> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("There")
            .field("T", &type_name::<T>())
            .finish()
    }
}

impl<T: Marker> Default for There<T> {
    #[inline]
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T: Marker> Clone for There<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Marker> Copy for There<T> {}

impl<T: Marker> Marker for There<T> {}

impl<T: IndexedMarker> IndexedMarker for There<T> {
    const INDEX: usize = T::INDEX + 1;
}

/// Allows for borrow of type `T` from a type-level list.
/// `T` must be present in the the type list.
/// `M` denotes the position of `T` type in the type list.
/// If the `T` is unique in the scope of the type list, `M` can be inferred by the compiler.
pub trait Contains<T, M: Marker> {
    fn get(&self) -> &T;
    fn get_mut(&mut self) -> &mut T;
}

/// `Cons` type lists can be used as complex marker types,
/// Zipping a collection of marker types allows for more complex operations on a tye list types
/// while requiring to state only single marker type parameter that would most of the time be aotomatically inferred by the compiler.
/// e.g. this marker type enables operation on the subsets of type lists, defined by `Subset` trait.
impl<M1: Marker, M2: Marker> Marker for Cons<M1, M2> {}

/// Wrapper type associating a value of type `T` with a marker type `M`.
/// Allows for storing value zipped with marker type,
/// denoting position of its related type (e.g. container from which `T` value was retrieved) in a type list.
#[derive(Debug, Default, Clone, Copy)]
pub struct Marked<T, M: Marker> {
    pub value: T,
    _marker: PhantomData<M>,
}

impl<T, M: Marker> Marked<T, M> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self {
            value,
            _marker: PhantomData,
        }
    }
}

impl<T, M: Marker> Deref for Marked<T, M> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T, M: Marker> DerefMut for Marked<T, M> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T: Destroy, M: Marker> Destroy for Marked<T, M> {
    type Context<'a> = T::Context<'a>;

    type DestroyError = T::DestroyError;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.value.destroy(context)
    }
}
