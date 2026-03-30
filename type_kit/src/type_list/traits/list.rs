mod bool;
mod optional;
mod subset;

pub use bool::*;
pub use optional::*;
pub use subset::*;

use crate::{Cons, Fin, Marker, Nil, TypedNil};

/// Trait definiing common operation and associated types for type-level lists.
pub trait TypeList: Sized {
    /// Length of the type list - number of `Cons` nodes in the type list.
    const LEN: usize;
    /// Type of the item stored in the `head` of the first `Cons` node of the type list implementing the trait.
    type Item;
    /// Type of the `tail` of the first `Cons` node of the type list implementing the trait.
    type Next: TypeList;
    /// Type list of borrowed references to the items in the type list.
    type RefList<'a>
    where
        Self: 'a;
    /// Type list of optional borrowed references to the items in the type list.
    type RefListOpt<'a>
    where
        Self: 'a;
    /// Type list of mutable references to the items in the type list.
    type MutList<'a>
    where
        Self: 'a;
    /// Type list of optional mutable references to the items in the type list.
    type MutListOpt<'a>
    where
        Self: 'a;
    /// Type list of optional owned items in the type list.
    type OptList;

    /// Returns the length of the type list.
    #[inline]
    fn len(&self) -> usize {
        Self::LEN
    }

    /// Returns `true` if the type list is empty.
    #[inline]
    fn is_empty(&self) -> bool {
        Self::LEN == 0
    }

    /// Converts borrow of the type list into a list of its items borrows.
    fn as_ref(&self) -> Self::RefList<'_>;

    /// Converts mutable borrow of the type list into a list of its items mutable borrows.
    fn as_mut(&mut self) -> Self::MutList<'_>;

    /// Constructs a new type list by wrapping 'Self` winth a `Cons` node holding `item` as its new head.
    #[inline]
    fn append<N>(self, item: N) -> Cons<N, Self> {
        Cons::new(item, self)
    }

    /// Converts borrow of the type list into a list containing a subset of its items borrows.
    #[inline]
    fn sub_ref<M: Marker, S: Subset<Self, M>>(&self) -> S::RefList<'_> {
        S::sub_get(self)
    }

    /// Converts mutable borrow of the type list into a list containing a subset of its items mutable borrows.
    ///
    /// # Safety
    /// User must ensure that the `S` subset list does not contain duplicate elements.
    /// Otherwise aliased mutable references may be created, leading to undefined behavior.
    #[inline]
    unsafe fn sub_mut<M: Marker, S: Subset<Self, M>>(&mut self) -> S::MutList<'_> {
        unsafe { S::sub_get_mut(self) }
    }

    /// Attempts to unwrap a list of optional borrowed references into a list of borrowed references.
    ///
    /// # Panics
    /// Panics if any of the items in the `RefListOpt` is `None`.
    fn unwrap_ref(opt: Self::RefListOpt<'_>) -> Self::RefList<'_>;

    /// Attempts to unwrap a list of optional mutable references into a list of mutable references.
    ///
    /// # Panics
    /// Panics if any of the items in the `MutListOpt` is `None`.
    fn unwrap_mut(opt: Self::MutListOpt<'_>) -> Self::MutList<'_>;

    /// Attempts to unwrap a list of optional owned items into a list of owned items.
    ///
    /// # Panics
    /// Panics if any of the items in the `OptList` is `None`.
    fn unwrap_owned(opt: Self::OptList) -> Self;
}

pub type RefList<'a, T> = <T as TypeList>::RefList<'a>;
pub type MutList<'a, T> = <T as TypeList>::MutList<'a>;
pub type RefListOpt<'a, T> = <T as TypeList>::RefListOpt<'a>;
pub type MutListOpt<'a, T> = <T as TypeList>::MutListOpt<'a>;
pub type OptList<T> = <T as TypeList>::OptList;

impl<N> TypeList for TypedNil<N> {
    const LEN: usize = 0;
    type Item = N;
    type Next = Self;
    type RefList<'a>
        = Self
    where
        N: 'a;
    type RefListOpt<'a>
        = Self
    where
        N: 'a;
    type MutList<'a>
        = Self
    where
        N: 'a;
    type MutListOpt<'a>
        = Self
    where
        N: 'a;
    type OptList = Self;

    #[inline]
    fn as_ref(&self) -> Self::RefList<'_> {
        *self
    }

    #[inline]
    fn as_mut(&mut self) -> Self::MutList<'_> {
        *self
    }

    #[inline]
    fn unwrap_ref<'a>(opt: Self::RefListOpt<'a>) -> Self::RefList<'a>
    where
        N: 'a,
    {
        opt
    }

    #[inline]
    fn unwrap_mut<'a>(opt: Self::MutListOpt<'a>) -> Self::MutList<'a>
    where
        N: 'a,
    {
        opt
    }

    #[inline]
    fn unwrap_owned(opt: Self::OptList) -> Self {
        opt
    }
}

impl<T> TypeList for Fin<T> {
    const LEN: usize = 1;
    type Item = T;
    type Next = Nil;
    type RefList<'a>
        = &'a T
    where
        T: 'a;
    type RefListOpt<'a>
        = Option<&'a T>
    where
        T: 'a;
    type MutList<'a>
        = &'a mut T
    where
        T: 'a;
    type MutListOpt<'a>
        = Option<&'a mut T>
    where
        T: 'a;
    type OptList = Option<T>;

    #[inline]
    fn as_ref(&self) -> Self::RefList<'_> {
        self
    }

    #[inline]
    fn as_mut(&mut self) -> Self::MutList<'_> {
        self
    }

    #[inline]
    fn unwrap_ref<'a>(opt: Self::RefListOpt<'a>) -> Self::RefList<'a> {
        opt.unwrap()
    }

    #[inline]
    fn unwrap_mut<'a>(opt: Self::MutListOpt<'a>) -> Self::MutList<'a> {
        opt.unwrap()
    }

    #[inline]
    fn unwrap_owned(opt: Self::OptList) -> Self {
        Fin::new(opt.unwrap())
    }
}

impl<T, N: TypeList> TypeList for Cons<T, N> {
    const LEN: usize = N::LEN + 1;
    type Item = T;
    type Next = N;
    type RefList<'a>
        = Cons<&'a Self::Item, N::RefList<'a>>
    where
        T: 'a,
        N: 'a;
    type RefListOpt<'a>
        = Cons<Option<&'a Self::Item>, N::RefListOpt<'a>>
    where
        T: 'a,
        N: 'a;
    type MutList<'a>
        = Cons<&'a mut Self::Item, N::MutList<'a>>
    where
        T: 'a,
        N: 'a;
    type MutListOpt<'a>
        = Cons<Option<&'a mut Self::Item>, N::MutListOpt<'a>>
    where
        T: 'a,
        N: 'a;
    type OptList = Cons<Option<T>, N::OptList>;

    #[inline]
    fn as_ref(&self) -> Self::RefList<'_> {
        Cons::new(&self.head, self.tail.as_ref())
    }

    #[inline]
    fn as_mut(&mut self) -> Self::MutList<'_> {
        Cons::new(&mut self.head, self.tail.as_mut())
    }

    #[inline]
    fn unwrap_ref<'a>(opt: Self::RefListOpt<'a>) -> Self::RefList<'a> {
        Cons::new(opt.head.unwrap(), N::unwrap_ref(opt.tail))
    }

    #[inline]
    fn unwrap_mut<'a>(opt: Self::MutListOpt<'a>) -> Self::MutList<'a> {
        Cons::new(opt.head.unwrap(), N::unwrap_mut(opt.tail))
    }

    #[inline]
    fn unwrap_owned(opt: Self::OptList) -> Self {
        Cons::new(opt.head.unwrap(), N::unwrap_owned(opt.tail))
    }
}

/// Marker trait for type list not capturing ant lifetime parameters.
pub trait StaticTypeList: TypeList + 'static {}

impl<T: TypeList + 'static> StaticTypeList for T {}

/// Marker traits denoting that a type list is non-empty.
/// It contains at least one `Cons` node, or is a `Fin` node.
pub trait NonEmptyList: TypeList {}

impl<C, N: NonEmptyList> NonEmptyList for Cons<C, N> {}

impl<H> NonEmptyList for Fin<H> {}

// TODO: Technically, Cons<Nil, Nil> would be marked as NonEmptyList
impl<T, C> NonEmptyList for Cons<C, TypedNil<T>> {}
