use std::ptr::NonNull;

use crate::{Cons, Contains, Marker, TypeList, TypedNil};

/// Defines `Self` as a subset of type-level list `L`.
/// Uses marker type `T`. to identify positions of subset elements in the superset list.
pub trait Subset<L, T: Marker>: TypeList {
    /// Convert borroww of the superset list into a list borrows for subset of its items.
    fn sub_get<'a>(superset: &'a L) -> Self::RefList<'a>;

    /// Convert mutable borroww of the superset list into a list of mutable borrows for subset of its items.
    ///
    /// # Safety
    /// User must ensure that the subset list does not contain duplicate elements.
    /// Otherwise aliased mutable references may be created, leading to undefined behavior.
    unsafe fn sub_get_mut(superset: &mut L) -> Self::MutList<'_>;

    /// Updates the supseset list items with the values from the subset list.
    fn sub_write(self, superset: &mut L);
}

/// Defines `Self` as a copyable subset of type-level list `L`.
pub trait SubsetCopy<L, T: Marker>: TypeList + Clone + Copy {
    /// Creates a copy of the subset of items from the superset list.
    fn sub_copy(superset: &L) -> Self;
}

impl<T: 'static, L, M: Marker> Subset<L, M> for TypedNil<T>
where
    L: Contains<TypedNil<T>, M>,
{
    #[inline]
    fn sub_get(_superset: &L) -> Self::RefList<'_> {
        TypedNil::new()
    }

    #[inline]
    unsafe fn sub_get_mut(_superset: &mut L) -> Self::MutList<'_> {
        TypedNil::new()
    }

    fn sub_write(self, _superset: &mut L) {}
}

impl<T: 'static, L, M: Marker> SubsetCopy<L, M> for TypedNil<T>
where
    L: Contains<TypedNil<T>, M>,
{
    #[inline]
    fn sub_copy(superset: &L) -> Self {
        *superset.get()
    }
}

/// Implements `Subset` for a type-level list, where `Cons<T, N>` is a subset of superset `L`
/// using marker types `M1` and `M2`. This allows extracting references to the subset elements
/// from the superset, ensuring that each subset element corresponds to a unique marker in the superset.
impl<T: 'static, L, M1: Marker, M2: Marker, N: Subset<L, M2>> Subset<L, Cons<M1, M2>> for Cons<T, N>
where
    L: Contains<T, M1>,
{
    #[inline]
    fn sub_get(superset: &L) -> Self::RefList<'_> {
        Cons::new(superset.get(), N::sub_get(superset))
    }

    #[inline]
    unsafe fn sub_get_mut(superset: &mut L) -> Self::MutList<'_> {
        let mut reborrow = unsafe { NonNull::new_unchecked(superset) };
        Cons::new(superset.get_mut(), unsafe {
            N::sub_get_mut(reborrow.as_mut())
        })
    }

    #[inline]
    fn sub_write(self, superset: &mut L) {
        let Cons { head, tail } = self;
        *superset.get_mut() = head;
        tail.sub_write(superset);
    }
}

impl<T: 'static + Clone + Copy, L, M1: Marker, M2: Marker, N: SubsetCopy<L, M2>>
    SubsetCopy<L, Cons<M1, M2>> for Cons<T, N>
where
    L: Contains<T, M1>,
{
    #[inline]
    fn sub_copy(superset: &L) -> Self {
        Cons::new(*superset.get(), N::sub_copy(superset))
    }
}

/// Defines `Self` as a superset of type-level list `L`.
/// Uses marker type `T`. to identify positions of subset elements in the superset list.
///
/// # Note
/// The methods as currently defined, are technically valid, but are not useful in practice
/// as the type inference will not be able to infer the correct type for `L` and `T`.
/// Its use requires to explicitly state the superset type in the method invocation,
/// making the syntax more complex and less ergonomic than directly using Subset methods
/// Temporary left here for future reference and potential improvements, or use as a marker trait.
pub trait Superset<L: TypeList, T: Marker> {
    fn super_get(&self) -> L::RefList<'_>;

    /// # Safety
    /// User must ensure that the subset list does not contain duplicate elements.
    /// Otherwise aliased mutable references may be created, leading to undefined behavior.
    unsafe fn super_get_mut(&mut self) -> L::MutList<'_>;
}

impl<T, L: TypeList, M: Marker> Superset<L, M> for T
where
    L: Subset<T, M>,
{
    #[inline]
    fn super_get(&self) -> L::RefList<'_> {
        L::sub_get(self)
    }

    #[inline]
    unsafe fn super_get_mut(&mut self) -> L::MutList<'_> {
        unsafe { L::sub_get_mut(self) }
    }
}
