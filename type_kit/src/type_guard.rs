#[cfg(test)]
pub(crate) mod test_types {
    use super::FromGuard;

    #[derive(Debug, Clone, Copy)]
    pub struct A(pub u32);

    impl FromGuard for A {
        type Inner = u32;

        fn into_inner(self) -> Self::Inner {
            self.0
        }

        unsafe fn from_inner(inner: Self::Inner) -> Self {
            Self(inner)
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub struct B(pub u32);

    impl FromGuard for B {
        type Inner = u32;

        fn into_inner(self) -> Self::Inner {
            self.0
        }

        unsafe fn from_inner(inner: Self::Inner) -> Self {
            Self(inner)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        any::{type_name, TypeId},
        collections::HashMap,
    };

    use crate::{
        list_value,
        type_guard::test_types::{A, B},
        unpack_list, Cons, FromGuard, Nil, TypeGuardConversionError,
    };

    #[test]
    fn test_type_guard_valid_conversion() {
        let a = A(42);
        let a_guard = a.into_guard();
        let a = A::try_from_guard(a_guard).unwrap();
        assert_eq!(a.0, 42);

        let b = B(42);
        let b_guard = b.into_guard();
        let b = B::try_from_guard(b_guard).unwrap();
        assert_eq!(b.0, 42);
    }

    #[test]
    #[cfg(debug_assertions)]
    fn test_type_guard_checks_type_in_debug_build() {
        let a = A(42);
        let a_guard = a.into_guard();
        assert!(B::try_from_guard(a_guard).is_err());

        let b = B(42);
        let b_guard = b.into_guard();
        assert!(A::try_from_guard(b_guard).is_err());
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn test_type_guard_skips_type_check_in_release_build() {
        let a = A(42);
        let a_guard = a.into_guard();
        let b_invalid = B::try_from_guard(a_guard).unwrap();
        assert_eq!(b_invalid.0, 42);
    }

    #[test]
    #[cfg(debug_assertions)]
    fn test_type_guard_error_value() {
        let b_type_name = type_name::<B>();
        let a_type_name = type_name::<A>();

        let a = A(42);
        let a_guard = a.into_guard();
        match B::try_from_guard(a_guard) {
            Err((guard, TypeGuardConversionError { to, from })) => {
                assert_eq!(to, b_type_name);
                assert_eq!(from, a_type_name);
                assert_eq!(guard.inner(), &42);
            }
            _ => assert!(false),
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    fn test_type_guard_error_display() {
        let b_type_name = format!("{:?}", type_name::<B>());
        let a_type_name = format!("{:?}", type_name::<A>());
        let a_type_id = format!("{:?}", TypeId::of::<A>());

        let a = A(42);
        let a_guard = a.into_guard();
        let error = B::try_from_guard(a_guard).unwrap_err();
        assert_eq!(
            error.1.to_string(),
            format!(
                "TypeGuard conversion error: cannot convert from {} to {}",
                if cfg!(debug_assertions) {
                    a_type_name
                } else {
                    a_type_id
                },
                b_type_name
            )
        );
    }

    #[test]
    fn test_type_guard_as_hash_map_index() {
        let a_1 = A(42).into_guard();
        let a_2 = A(31).into_guard();
        let mut map = HashMap::new();
        map.insert(a_1, 42);
        map.insert(a_2, 31);
        assert_eq!(map.get(&a_1).unwrap(), &42);
        assert_eq!(map.get(&a_2).unwrap(), &31);
    }

    #[test]
    fn test_type_guard_type_list_conversion() {
        let a_1 = A(42);
        let a_2 = A(43);
        let b_1 = B(31);
        let b_2 = B(32);
        let list_guard = list_value!(a_1, b_1, a_2, b_2, Nil::new()).into_guard();
        let list = Cons::<A, Cons<B, Cons<A, Cons<B, Nil>>>>::try_from_guard(list_guard).unwrap();
        let unpack_list![a_1, b_1, a_2, b_2, _nil] = list;
        assert_eq!(a_1.0, 42);
        assert_eq!(b_1.0, 31);
        assert_eq!(a_2.0, 43);
        assert_eq!(b_2.0, 32);
    }
}

use std::{
    any::{type_name, TypeId},
    error::Error,
    fmt::{Debug, Display, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use crate::{Cons, Destroy, DestroyResult, TypedNil};

pub type Valid<T> = TypeGuardUnlocked<<T as FromGuard>::Inner, T>;
pub type ValidRef<'a, T> = TypeGuardUnlockedRef<'a, <T as FromGuard>::Inner, T>;
pub type ValidMut<'a, T> = TypeGuardUnlockedMut<'a, <T as FromGuard>::Inner, T>;
pub type Guard<T> = TypeGuard<<T as FromGuard>::Inner>;
pub type GuardResult<T> = Result<T, (Guard<T>, TypeGuardConversionError)>;

pub trait FromGuard: 'static + Sized {
    type Inner;

    fn into_inner(self) -> Self::Inner;

    unsafe fn from_inner(inner: Self::Inner) -> Self;

    #[inline]
    fn try_from_guard(value: Guard<Self>) -> GuardResult<Self> {
        let value: Conv<Self> = value.try_into()?;
        Ok(value.unwrap())
    }

    #[inline]
    fn check_type(value: &Guard<Self>) -> Result<(), TypeGuardConversionError> {
        value.check_type::<Self>()
    }

    #[inline]
    fn into_guard(self) -> Guard<Self> {
        unsafe { TypeGuard::from_inner::<Self>(self.into_inner()) }
    }
}

impl<T: FromGuard> Valid<T> {
    #[inline]
    fn into(self) -> T {
        unsafe { T::from_inner(self.into_inner()) }
    }
}

pub trait IntoOuter<T>: Sized {
    fn try_into_outer(self) -> Result<T, (Self, TypeGuardConversionError)>;
}

impl<T: FromGuard> IntoOuter<T> for Guard<T> {
    #[inline]
    fn try_into_outer(self) -> Result<T, (Self, TypeGuardConversionError)> {
        T::try_from_guard(self)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Conv<T: FromGuard>(T);

impl<T: FromGuard> Conv<T> {
    #[inline]
    pub fn unwrap(self) -> T {
        self.0
    }
}

impl<T: FromGuard> TryFrom<Guard<T>> for Conv<T> {
    type Error = (Guard<T>, TypeGuardConversionError);

    fn try_from(value: Guard<T>) -> Result<Self, Self::Error> {
        let unlocked: Valid<T> = value.try_into()?;
        Ok(Conv(unlocked.into()))
    }
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy)]
pub struct GuardType {
    type_id: TypeId,
    type_name: &'static str,
}

#[cfg(debug_assertions)]
impl PartialEq for GuardType {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id
    }
}

#[cfg(debug_assertions)]
impl Eq for GuardType {}

#[cfg(debug_assertions)]
impl Hash for GuardType {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
    }
}

#[cfg(debug_assertions)]
impl GuardType {
    #[inline]
    pub fn new<T: 'static>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
        }
    }
}

#[cfg(debug_assertions)]
impl GuardType {
    #[inline]
    fn check_type<U: 'static>(&self) -> Result<(), TypeGuardConversionError> {
        if TypeId::of::<U>() != self.type_id {
            return Err(TypeGuardConversionError {
                to: type_name::<U>(),
                from: self.type_name,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct TypeGuard<T> {
    inner: T,
    #[cfg(debug_assertions)]
    guard_type: GuardType,
}

impl<T> Debug for TypeGuard<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug_struct = f.debug_struct(&format!("TypeGuard<{}>", type_name::<T>()));
        #[cfg(debug_assertions)]
        let debug_struct = debug_struct.field("guard_type", &self.guard_type);
        debug_struct.finish()
    }
}

#[cfg(debug_assertions)]
impl<T: PartialEq> PartialEq for TypeGuard<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.guard_type == other.guard_type && self.inner == other.inner
    }
}

#[cfg(not(debug_assertions))]
impl<T: PartialEq> PartialEq for TypeGuard<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T: Eq> Eq for TypeGuard<T> {}

impl<T: Hash> Hash for TypeGuard<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        #[cfg(debug_assertions)]
        self.guard_type.hash(state);
        self.inner.hash(state);
    }
}

impl<I> TypeGuard<I> {
    /// Creates a new `TypeGuard` from the inner value `T::Inner`.
    ///
    /// # Safety
    ///
    /// The `from_inner` method is marked as `unsafe` because there is no way to ensure at compile-time
    /// that the `inner` value passed here was indeed constructed from an instance of the type `T`.
    /// While multiple types can share the same inner type (`T::Inner`), the inner type alone is not
    /// enough to uniquely determine the outer type `T`. This can lead to situations where an inner
    /// value is incorrectly associated with the wrong type `T`, which could cause undefined behavior
    /// when the `TypeGuard` is used.
    #[inline]
    pub unsafe fn from_inner<T: FromGuard<Inner = I>>(inner: T::Inner) -> Self {
        Self {
            inner,
            #[cfg(debug_assertions)]
            guard_type: GuardType::new::<T>(),
        }
    }

    #[cfg(debug_assertions)]
    #[inline]
    pub fn type_id(&self) -> TypeId {
        self.guard_type.type_id
    }

    #[cfg(debug_assertions)]
    #[inline]
    pub fn type_name(&self) -> &'static str {
        self.guard_type.type_name
    }

    #[inline]
    pub fn inner(&self) -> &I {
        &self.inner
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut I {
        &mut self.inner
    }

    #[inline]
    pub fn check_type<F: FromGuard<Inner = I>>(&self) -> Result<(), TypeGuardConversionError> {
        #[cfg(debug_assertions)]
        self.guard_type.check_type::<F>()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TypeGuardUnlocked<T, U: 'static> {
    inner: T,
    _phantom: PhantomData<U>,
}

impl<T, U: 'static> TypeGuardUnlocked<T, U> {
    #[inline]
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T, U> TryFrom<TypeGuard<T>> for TypeGuardUnlocked<T, U> {
    type Error = (TypeGuard<T>, TypeGuardConversionError);

    #[inline]
    fn try_from(value: TypeGuard<T>) -> Result<Self, Self::Error> {
        #[cfg(debug_assertions)]
        {
            if let Err(err) = value.guard_type.check_type::<U>() {
                return Err((value, err));
            }
        }
        Ok(TypeGuardUnlocked {
            inner: value.inner,
            _phantom: PhantomData,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TypeGuardUnlockedRef<'a, T, U: 'static> {
    inner: &'a T,
    _phantom: PhantomData<U>,
}

impl<'a, T, U: 'static> TypeGuardUnlockedRef<'a, T, U> {
    #[inline]
    pub fn inner_ref(self) -> &'a T {
        self.inner
    }
}

impl<'a, T, U> TryFrom<&'a TypeGuard<T>> for TypeGuardUnlockedRef<'a, T, U> {
    type Error = TypeGuardConversionError;

    #[inline]
    fn try_from(value: &'a TypeGuard<T>) -> Result<Self, Self::Error> {
        #[cfg(debug_assertions)]
        value.guard_type.check_type::<U>()?;
        Ok(TypeGuardUnlockedRef {
            inner: &value.inner,
            _phantom: PhantomData,
        })
    }
}

#[derive(Debug)]
pub struct TypeGuardUnlockedMut<'a, T, U: 'static> {
    inner: &'a mut T,
    _phantom: PhantomData<U>,
}

impl<'a, T, U: 'static> TypeGuardUnlockedMut<'a, T, U> {
    #[inline]
    pub fn inner_ref(self) -> &'a T {
        self.inner
    }

    #[inline]
    pub fn inner_mut(self) -> &'a mut T {
        self.inner
    }
}

impl<'a, T, U> TryFrom<&'a mut TypeGuard<T>> for TypeGuardUnlockedMut<'a, T, U> {
    type Error = TypeGuardConversionError;

    #[inline]
    fn try_from(value: &'a mut TypeGuard<T>) -> Result<Self, Self::Error> {
        #[cfg(debug_assertions)]
        value.guard_type.check_type::<U>()?;
        Ok(TypeGuardUnlockedMut {
            inner: &mut value.inner,
            _phantom: PhantomData,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TypeGuardConversionError {
    to: &'static str,
    from: &'static str,
}

impl Display for TypeGuardConversionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TypeGuard conversion error: cannot convert from {:?} to {:?}",
            self.from, self.to
        )
    }
}

impl Error for TypeGuardConversionError {}

impl<T: Destroy> Destroy for TypeGuard<T> {
    type Context<'a> = T::Context<'a>;
    type DestroyError = T::DestroyError;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.inner.destroy(context)
    }
}

impl<T: 'static> FromGuard for TypedNil<T> {
    type Inner = Self;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        inner
    }
}

impl<T: FromGuard, N: FromGuard> FromGuard for Cons<T, N> {
    type Inner = Cons<T::Inner, N::Inner>;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Cons {
            head: self.head.into_inner(),
            tail: self.tail.into_inner(),
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Cons {
            head: T::from_inner(inner.head),
            tail: N::from_inner(inner.tail),
        }
    }
}

pub type GuardListResult<T, L> = Result<T, (L, TypeGuardConversionError)>;

pub trait GuardList: Sized {
    type Guard;

    fn try_from_guard(guard: Self::Guard) -> GuardListResult<Self, Self::Guard>;

    fn into_guard(self) -> Self::Guard;
}

impl<T> GuardList for TypedNil<T> {
    type Guard = TypedNil<T>;

    #[inline]
    fn try_from_guard(_: Self::Guard) -> GuardListResult<Self, Self::Guard> {
        Ok(TypedNil::new())
    }

    #[inline]
    fn into_guard(self) -> Self::Guard {
        self
    }
}

impl<T: FromGuard, N: GuardList> GuardList for Cons<T, N> {
    type Guard = Cons<Guard<T>, N::Guard>;

    #[inline]
    fn try_from_guard(guard: Self::Guard) -> GuardListResult<Self, Self::Guard> {
        let Cons { head, tail } = guard;
        match T::check_type(&head) {
            Err(err) => Err((Cons { head, tail }, err)),
            Ok(()) => match N::try_from_guard(tail) {
                Err((tail, err)) => Err((Cons { head, tail }, err)),
                Ok(tail) => Ok(Cons {
                    head: T::try_from_guard(head).unwrap(),
                    tail,
                }),
            },
        }
    }

    #[inline]
    fn into_guard(self) -> Self::Guard {
        Cons {
            head: self.head.into_guard(),
            tail: self.tail.into_guard(),
        }
    }
}

#[cfg(test)]
mod test_type_gurad_list {
    use super::{
        test_types::{A, B},
        GuardList,
    };
    use crate::{
        list_value,
        type_list::{Cons, Nil},
        unpack_list,
    };

    // type GuardList = guard_list![A, B, A, B];

    #[test]
    fn test_type_guard_list_success() {
        let a_1 = A(42);
        let a_2 = A(43);
        let b_1 = B(31);
        let b_2 = B(32);
        let list_guard = list_value!(a_1, b_1, a_2, b_2, Nil::new()).into_guard();
        let list = Cons::<A, Cons<B, Cons<A, Cons<B, Nil>>>>::try_from_guard(list_guard).unwrap();
        let unpack_list![a_1, b_1, a_2, b_2, _nil] = list;
        assert_eq!(a_1.0, 42);
        assert_eq!(b_1.0, 31);
        assert_eq!(a_2.0, 43);
        assert_eq!(b_2.0, 32);
    }
}
