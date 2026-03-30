use std::{
    any::type_name,
    fmt::{Debug, Formatter},
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{Destroy, DestroyResult, Marker};

/// Marker denoting the target position in the type list
/// Terminates the marker chain.
#[derive(Debug, Default, Clone, Copy)]
pub struct Here {}

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
