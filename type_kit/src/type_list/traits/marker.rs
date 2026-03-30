use crate::{Cons, Here, There};

/// Marker trait for type list type position identification.
pub trait Marker: 'static + Clone + Copy + Send + Sync {}

/// Appends index value to the simple `Marker` types in which
/// linear position in the type list is known at compile time.
/// e.g. `Here`, There<Here>, There<There<Here>>, etc.
pub trait IndexedMarker: Marker {
    const INDEX: usize;
}

impl Marker for Here {}

impl IndexedMarker for Here {
    const INDEX: usize = 0;
}

impl<T: Marker> Marker for There<T> {}

impl<T: IndexedMarker> IndexedMarker for There<T> {
    const INDEX: usize = T::INDEX + 1;
}

/// `Cons` type lists can be used as complex marker types,
/// Zipping a collection of marker types allows for more complex operations on a tye list types
/// while requiring to state only single marker type parameter that would most of the time be aotomatically inferred by the compiler.
/// e.g. this marker type enables operation on the subsets of type lists, defined by `Subset` trait.
impl<M1: Marker, M2: Marker> Marker for Cons<M1, M2> {}
