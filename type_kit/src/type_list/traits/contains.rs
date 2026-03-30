use crate::{Cons, Fin, Here, Marker, There, TypedNil};

/// Allows for borrow of type `T` from a type-level list.
/// `T` must be present in the the type list.
/// `M` denotes the position of `T` type in the type list.
/// If the `T` is unique in the scope of the type list, `M` can be inferred by the compiler.
pub trait Contains<T, M: Marker> {
    fn get(&self) -> &T;
    fn get_mut(&mut self) -> &mut T;
}

impl<T> Contains<TypedNil<T>, Here> for TypedNil<T> {
    #[inline]
    fn get(&self) -> &TypedNil<T> {
        self
    }

    #[inline]
    fn get_mut(&mut self) -> &mut TypedNil<T> {
        self
    }
}

impl<H> Contains<H, Here> for Fin<H> {
    #[inline]
    fn get(&self) -> &H {
        &self.head
    }

    #[inline]
    fn get_mut(&mut self) -> &mut H {
        &mut self.head
    }
}

impl<S, N> Contains<S, Here> for Cons<S, N> {
    #[inline]
    fn get(&self) -> &S {
        &self.head
    }

    #[inline]
    fn get_mut(&mut self) -> &mut S {
        &mut self.head
    }
}

impl<O, S, T: Marker, N: Contains<S, T>> Contains<S, There<T>> for Cons<O, N> {
    #[inline]
    fn get(&self) -> &S {
        self.tail.get()
    }

    #[inline]
    fn get_mut(&mut self) -> &mut S {
        self.tail.get_mut()
    }
}
