use crate::{Cons, TypedNil};

/// Defines boolean operations over type-level lists containing `bool` or `Option<T>` items.
pub trait BoolList {
    /// Returns `true` if all items in the list are `true` or `Some`.
    fn all(&self) -> bool;
    /// Returns `true` if any item in the list is `true` or `Some`.
    fn any(&self) -> bool;
    /// Returns `true` if no items in the list are `true` or `Some`.
    fn none(&self) -> bool;
}

impl<T: 'static> BoolList for TypedNil<T> {
    #[inline]
    fn all(&self) -> bool {
        true
    }

    #[inline]
    fn any(&self) -> bool {
        false
    }

    #[inline]
    fn none(&self) -> bool {
        true
    }
}

impl<T> BoolList for Option<T> {
    #[inline]
    fn all(&self) -> bool {
        self.is_some()
    }

    #[inline]
    fn any(&self) -> bool {
        self.is_some()
    }

    #[inline]
    fn none(&self) -> bool {
        self.is_none()
    }
}

impl<N: BoolList> BoolList for Cons<bool, N> {
    #[inline]
    fn all(&self) -> bool {
        self.head && self.tail.all()
    }

    #[inline]
    fn any(&self) -> bool {
        self.head || self.tail.any()
    }

    #[inline]
    fn none(&self) -> bool {
        !self.head && self.tail.none()
    }
}

impl<C, N: BoolList> BoolList for Cons<Option<C>, N> {
    #[inline]
    fn all(&self) -> bool {
        self.head.is_some() && N::all(&self.tail)
    }

    #[inline]
    fn any(&self) -> bool {
        self.head.is_some() || N::any(&self.tail)
    }

    #[inline]
    fn none(&self) -> bool {
        self.head.is_none() && N::none(&self.tail)
    }
}
