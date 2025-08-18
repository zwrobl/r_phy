use std::{any::type_name, marker::PhantomData, mem::ManuallyDrop};

use crate::{Here, Marker, There, TypedNil};

#[derive(Clone, Copy)]
pub union UCons<T, U: UnionList> {
    head: ManuallyDrop<T>,
    tail: ManuallyDrop<U>,
}

#[derive(Clone, Copy)]
pub union UFin<T> {
    head: ManuallyDrop<T>,
}

pub trait UnionList {
    const LEN: usize;
    type Item;
    type Next: UnionList;
}

impl<T> UnionList for UFin<T> {
    const LEN: usize = 1;
    type Item = T;
    type Next = Self;
}

impl<T> UnionList for TypedNil<T> {
    const LEN: usize = 0;
    type Item = T;
    type Next = Self;
}

impl<T, U: UnionList> UnionList for UCons<T, U> {
    const LEN: usize = 1 + U::LEN;
    type Item = T;
    type Next = U;
}

pub trait UContains<C, M: Marker>: UnionList {
    fn new(value: C) -> Self;
    /// # Safety
    /// User must ensure that the union contains C variant
    unsafe fn take(self) -> C;
    /// # Safety
    /// User must ensure that the union contains C variant
    unsafe fn get(&self) -> &C;
    /// # Safety
    /// User must ensure that the union contains C variant
    unsafe fn get_mut(&mut self) -> &mut C;
    /// # Safety
    /// User must ensure that the union contains C variant
    unsafe fn drop_variant(&mut self);
}

impl<C> UContains<C, Here> for UFin<C> {
    fn new(value: C) -> Self {
        let head = ManuallyDrop::new(value);
        Self { head }
    }

    unsafe fn take(mut self) -> C {
        unsafe { ManuallyDrop::take(&mut self.head) }
    }

    unsafe fn get(&self) -> &C {
        unsafe { &self.head }
    }

    unsafe fn get_mut(&mut self) -> &mut C {
        unsafe { &mut self.head }
    }

    unsafe fn drop_variant(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.head) }
    }
}

impl<C, N: UnionList> UContains<C, Here> for UCons<C, N> {
    fn new(value: C) -> Self {
        let head = ManuallyDrop::new(value);
        Self { head }
    }

    unsafe fn take(mut self) -> C {
        unsafe { ManuallyDrop::take(&mut self.head) }
    }

    unsafe fn get(&self) -> &C {
        unsafe { &self.head }
    }

    unsafe fn get_mut(&mut self) -> &mut C {
        unsafe { &mut self.head }
    }

    unsafe fn drop_variant(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.head) };
    }
}

impl<T, C, M: Marker, N: UContains<C, M>> UContains<C, There<M>> for UCons<T, N> {
    fn new(value: C) -> Self {
        let tail = ManuallyDrop::new(N::new(value));
        Self { tail }
    }

    unsafe fn take(mut self) -> C {
        unsafe { ManuallyDrop::take(&mut self.tail).take() }
    }

    unsafe fn get(&self) -> &C {
        unsafe { self.tail.get() }
    }

    unsafe fn get_mut(&mut self) -> &mut C {
        unsafe { self.tail.get_mut() }
    }

    unsafe fn drop_variant(&mut self) {
        unsafe { self.tail.drop_variant() }
    }
}

#[derive(Debug)]
pub struct UnionGuard<C, U: UnionList> {
    data: Option<U>,
    _marker: PhantomData<C>,
}

impl<C, U: UnionList> Drop for UnionGuard<C, U> {
    #[inline]
    fn drop(&mut self) {
        if self.data.is_some() {
            // As for drop_guard::DropGuard, panic on drop is problematic as this may happen during stack
            // unwinding from returning error, or another panic, obscuring the original error,
            // for now keep this here for testing purposes, later consider changing to log
            // or to log in Release builds
            panic!(
                "Dropping UnionGuard while inner {:?} value still exists",
                type_name::<C>()
            );
        }
    }
}

impl<C, U: UnionList> UnionGuard<C, U> {
    #[inline]
    pub fn new<M: Marker>(data: C) -> UnionGuard<C, U>
    where
        U: UContains<C, M>,
    {
        UnionGuard {
            data: Some(U::new(data)),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn get<M: Marker>(&self) -> &C
    where
        U: UContains<C, M>,
    {
        unsafe { self.data.as_ref().unwrap().get() }
    }

    #[inline]
    pub fn get_mut<M: Marker>(&mut self) -> &mut C
    where
        U: UContains<C, M>,
    {
        unsafe { self.data.as_mut().unwrap().get_mut() }
    }

    #[inline]
    pub fn drop_variant<M: Marker>(&mut self)
    where
        U: UContains<C, M>,
    {
        let mut data = self.data.take().unwrap();
        unsafe { data.drop_variant() }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UnionValue<C: Copy, U: UnionList + Copy> {
    data: U,
    _marker: PhantomData<C>,
}

impl<C: Copy, U: UnionList + Copy> UnionValue<C, U> {
    #[inline]
    pub fn new<M: Marker>(data: C) -> UnionValue<C, U>
    where
        U: UContains<C, M>,
    {
        UnionValue {
            data: U::new(data),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn get<M: Marker>(&self) -> &C
    where
        U: UContains<C, M>,
    {
        unsafe { self.data.get() }
    }

    #[inline]
    pub fn get_mut<M: Marker>(&mut self) -> &mut C
    where
        U: UContains<C, M>,
    {
        unsafe { self.data.get_mut() }
    }
}

#[macro_export]
macro_rules! ulist_type {
    [$head:ty, $tail:ty] => {
        UCons<$head, UFin<$tail>>
    };
    [$head:ty $(, $tail:ty)*] => {
        UCons<$head, ulist_type![$($tail),*]>
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    type UnionListCopyType = ulist_type![i32, f64, [u8; 128]];

    #[test]
    fn test_union_value() {
        let mut guard = UnionValue::<_, UnionListCopyType>::new(42.0f64);
        assert_eq!(*guard.get(), 42.0f64);
        *guard.get_mut() = 5.0f64;
        assert_eq!(*guard.get(), 5.0f64);
        assert_eq!(size_of_val(&guard), size_of::<[u8; 128]>());
    }

    type UnionListType = ulist_type![i32, String, [u8; 128]];

    #[test]
    #[should_panic]
    fn test_union_guard_panic_on_drop() {
        let guard = UnionGuard::<_, UnionListType>::new("Hello".to_string());
        assert_eq!(*guard.get(), "Hello");
    }

    #[test]
    fn test_union_guard_manual_drop() {
        let mut guard = UnionGuard::<_, UnionListType>::new("Hello".to_string());
        assert_eq!(*guard.get(), "Hello");
        *guard.get_mut() = "World".to_string();
        assert_eq!(*guard.get(), "World");
        guard.drop_variant();
    }
}
