use std::{
    any::type_name,
    convert::Infallible,
    error::Error,
    fmt::{Debug, Display, Formatter},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::{Create, CreateResult, Destroy, DestroyResult};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains() {
        let list = Nil::new().append(3.14).append(42).append("Item");
        let i32_item = list.get::<i32, _>();
        let f32_item = list.get::<f32, _>();
        let str_item = list.get::<&str, _>();
        assert_eq!(*i32_item, 42);
        assert_eq!(*f32_item, 3.14);
        assert_eq!(*str_item, "Item");
    }

    #[test]
    fn test_type_list_len() {
        let list = Nil::new().append(3.14).append(42).append("Item");
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_append() {
        let list = Nil::new().append(3.14).append(42).append("Item");
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_nil_types_are_empty() {
        let nil = Nil::new();
        assert!(nil.is_empty());
        assert_eq!(nil.len(), 0);
    }
}

pub trait Marker: 'static {}

#[derive(Debug, Default, Clone, Copy)]
pub struct Here {}

impl Marker for Here {}

pub struct There<T> {
    _phantom: PhantomData<T>,
}

impl<T> Debug for There<T> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("There")
            .field("T", &type_name::<T>())
            .finish()
    }
}

impl<T> Default for There<T> {
    #[inline]
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T> Clone for There<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for There<T> {}

impl<T: 'static> Marker for There<T> {}

pub trait Contains<T, M: Marker> {
    fn get(&self) -> &T;
    fn get_mut(&mut self) -> &mut T;
}

impl<M1: Marker, M2: Marker> Marker for Cons<M1, M2> {}

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

pub struct TypedNil<T> {
    _phantom: PhantomData<T>,
}

impl<T> Debug for TypedNil<T> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("TypedNil")
            .field("T", &type_name::<T>())
            .finish()
    }
}

impl<T> Clone for TypedNil<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TypedNil<T> {}

impl<T> Default for TypedNil<T> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T> PartialEq for TypedNil<T> {
    #[inline]
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T> Eq for TypedNil<T> {}

impl<T> TypedNil<T> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

pub type Nil = TypedNil<()>;

#[derive(Debug, Default, Clone, Copy)]
pub struct Fin<H> {
    pub head: H,
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

impl<H> Fin<H> {
    #[inline]
    pub fn new(head: H) -> Self {
        Self { head }
    }
}

impl<H> Deref for Fin<H> {
    type Target = H;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.head
    }
}

impl<H> DerefMut for Fin<H> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.head
    }
}

impl<H: PartialEq> PartialEq for Fin<H> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.head == other.head
    }
}

impl<H: Eq> Eq for Fin<H> {}

#[derive(Debug, Default, Clone, Copy)]
pub struct Cons<H, T> {
    pub head: H,
    pub tail: T,
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

impl<H, T> Cons<H, T> {
    #[inline]
    pub fn new(head: H, tail: T) -> Self {
        Self { head, tail }
    }

    #[inline]
    pub fn get<S, M: Marker>(&self) -> &S
    where
        Self: Contains<S, M>,
    {
        <Self as Contains<S, M>>::get(self)
    }

    #[inline]
    pub fn get_mut<S, M: Marker>(&mut self) -> &mut S
    where
        Self: Contains<S, M>,
    {
        <Self as Contains<S, M>>::get_mut(self)
    }
}

impl<H, T> Deref for Cons<H, T> {
    type Target = H;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.head
    }
}

impl<H, T> DerefMut for Cons<H, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.head
    }
}

impl<H: PartialEq, T: PartialEq> PartialEq for Cons<H, T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.head == other.head && self.tail == other.tail
    }
}

impl<H: Eq, T: Eq> Eq for Cons<H, T> {}

pub trait TypeList: Sized + 'static {
    const LEN: usize;
    type Item;
    type Next: TypeList;
    type RefList<'a>;
    type MutList<'a>;

    #[inline]
    fn len(&self) -> usize {
        Self::LEN
    }

    #[inline]
    fn is_empty(&self) -> bool {
        Self::LEN == 0
    }

    #[inline]
    fn append<N>(self, item: N) -> Cons<N, Self> {
        Cons::new(item, self)
    }
}

impl<N: 'static> TypeList for TypedNil<N> {
    const LEN: usize = 0;
    type Item = N;
    type Next = Self;
    type RefList<'a> = &'a Self;
    type MutList<'a> = &'a mut Self;
}

impl<T: 'static> TypeList for Fin<T> {
    const LEN: usize = 1;
    type Item = T;
    type Next = Nil;
    type RefList<'a> = &'a Self;
    type MutList<'a> = &'a mut Self;
}

impl<T: 'static, N: TypeList> TypeList for Cons<T, N> {
    const LEN: usize = N::LEN + 1;
    type Item = T;
    type Next = N;
    type RefList<'a> = Cons<&'a Self::Item, N::RefList<'a>>;
    type MutList<'a> = Cons<&'a mut Self::Item, N::MutList<'a>>;
}

pub type ListRefType<'a, T> = <T as TypeList>::RefList<'a>;
pub type ListMutType<'a, T> = <T as TypeList>::MutList<'a>;

#[cfg(test)]
mod test_macro {
    use crate::{list_type, list_value, unpack_any, Cons, Nil};

    trait AssertEqualTypes<A, B> {}

    impl<T> AssertEqualTypes<T, T> for () {}

    #[test]
    fn test_type_list_macro_generates_correct_type() {
        type GeneratedList = list_type![u8, u16, u32, Nil];
        type ExpectedList = Cons<u8, Cons<u16, Cons<u32, Nil>>>;

        // Compile-time assertion to check if the types are the same
        let _: &dyn AssertEqualTypes<GeneratedList, ExpectedList> = &();
    }

    #[test]
    fn text_list_macro_generates_correct_value() {
        let list = list_value![8u8, 16u16, 32u32, Nil::new()];
        let expected_list = Cons::new(8u8, Cons::new(16u16, Cons::new(32u32, Nil::new())));

        assert_eq!(list, expected_list);
    }

    #[test]
    fn test_unpack_list_macro() {
        let list = list_value![8u8, 16u16, 32u32];
        let unpack_any![value_u8, value_u16, value_u32] = list;

        assert_eq!(value_u8, 8u8);
        assert_eq!(value_u16, 16u16);
        assert_eq!(value_u32, 32u32);
    }
}

#[macro_export]
macro_rules! list_type {
    [$head:ty, $tail:ty] => {
        Cons<$head, $tail>
    };
    [$head:ty $(, $tail:ty)*] => {
        Cons<$head, list_type![$($tail),*]>
    };
}

#[macro_export]
macro_rules! list_value {
    [$head:expr, $tail:expr] => {
        Cons::new($head, $tail)
    };
    [$head:expr $(, $tail:expr)*] => {
        Cons::new($head, list_value![$($tail),*])
    };
}

#[macro_export]
macro_rules! unpack_list {
    [$tail:ident] => {
        Cons {
            head: $tail,
            ..
        }
    };
    [$head:ident $(, $tail:ident)*] => {
        Cons {
            head: $head,
            tail: unpack_list![$($tail),*]
        }
    };
}

#[macro_export]
macro_rules! unpack_any {
    [$tail:ident] => {
        $tail
    };
    [$head:ident $(, $tail:ident)*] => {
        Cons {
            head: $head,
            tail: unpack_any![$($tail),*]
        }
    };
}

impl<T: Create> Create for TypedNil<T> {
    type Config<'a> = ();
    type CreateError = Infallible;

    #[inline]
    fn create<'a, 'b>(_: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
        Ok(TypedNil::new())
    }
}

impl<T: Destroy> Destroy for TypedNil<T> {
    type Context<'a> = T::Context<'a>;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, _: Self::Context<'a>) -> DestroyResult<Self> {
        Ok(())
    }
}

impl<T: Create> Create for Fin<T> {
    type Config<'a> = T::Config<'a>;
    type CreateError = T::CreateError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        Ok(Fin::new(T::create(config, context)?))
    }
}

impl<T: Destroy> Destroy for Fin<T> {
    type Context<'a> = T::Context<'a>;
    type DestroyError = T::DestroyError;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.head.destroy(context)
    }
}

pub enum ConsCreateError<H: Create, T: Create> {
    Head(H::CreateError),
    Tail(T::CreateError),
}

impl<H: Create, T: Create> Debug for ConsCreateError<H, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Head(arg0) => f.debug_tuple("Head").field(arg0).finish(),
            Self::Tail(arg0) => f.debug_tuple("Tail").field(arg0).finish(),
        }
    }
}

impl<H: Create, T: Create> Display for ConsCreateError<H, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Head(arg0) => write!(f, "Head({})", arg0),
            Self::Tail(arg0) => write!(f, "Tail({})", arg0),
        }
    }
}

impl<H: Create, T: Create> Error for ConsCreateError<H, T> {}

impl<H: Create, T: Create> Create for Cons<H, T>
where
    for<'a> H::Context<'a>: Clone + Copy,
    for<'a> T: Destroy<Context<'a> = H::Context<'a>>,
{
    type Config<'a> = Cons<H::Config<'a>, T::Config<'a>>;
    type CreateError = ConsCreateError<H, T>;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let Cons { head, tail } = config;
        let head = H::create(head, context).map_err(|err| ConsCreateError::Head(err))?;
        let tail = T::create(tail, context).map_err(|err| ConsCreateError::Tail(err))?;
        Ok(Cons::new(head, tail))
    }
}

pub enum ConsDestroyError<H: Destroy, T: Destroy> {
    Head(H::DestroyError),
    Tail(T::DestroyError),
}

impl<H: Destroy, T: Destroy> Debug for ConsDestroyError<H, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Head(arg0) => f.debug_tuple("Head").field(arg0).finish(),
            Self::Tail(arg0) => f.debug_tuple("Tail").field(arg0).finish(),
        }
    }
}

impl<H: Destroy, T: Destroy> Display for ConsDestroyError<H, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Head(arg0) => write!(f, "Head({})", arg0),
            Self::Tail(arg0) => write!(f, "Tail({})", arg0),
        }
    }
}

impl<H: Destroy, T: Destroy> Error for ConsDestroyError<H, T> {}

impl<H: Destroy, T: Destroy> Destroy for Cons<H, T>
where
    for<'a> H::Context<'a>: Clone + Copy,
    for<'a> T: Destroy<Context<'a> = H::Context<'a>>,
{
    type Context<'a> = T::Context<'a>;
    type DestroyError = ConsDestroyError<H, T>;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.head
            .destroy(context)
            .map_err(|err| ConsDestroyError::Head(err))?;
        self.tail
            .destroy(context)
            .map_err(|err| ConsDestroyError::Tail(err))?;
        Ok(())
    }
}

#[cfg(test)]
mod test_type_list_create_destroy {
    use super::*;
    use crate::drop_guard::test_types::{FaillingCreate, FaillingDestroy, A, C};
    use crate::drop_guard::{Create, Destroy};

    type TestTypeList = list_type![A, A, A, A, A, TypedNil<A>];
    type TestTypeListFailingCreate = list_type![A, A, FaillingCreate, A, A, TypedNil<A>];
    type TestTypeListFailingDestroy = list_type![A, A, FaillingDestroy, A, A, TypedNil<A>];

    #[test]
    fn test_type_list_create_and_destroy() {
        let c = C {};
        let config_list = list_value![1u32, 2u32, 3u32, 4u32, 5u32, ()];
        let result = TestTypeList::create(config_list, &mut &c);
        assert!(result.is_ok());
        let result = result.unwrap().destroy(&mut &c);
        assert!(result.is_ok());
    }

    #[test]
    fn test_type_list_create_failure() {
        let c = C {};
        let config_list = list_value![1u32, 2u32, (), 4u32, 5u32, ()];
        let result = TestTypeListFailingCreate::create(config_list, &mut &c);
        assert!(matches!(
            result,
            Err(ConsCreateError::Tail(ConsCreateError::Tail(
                ConsCreateError::Head(_)
            )))
        ));
    }

    #[test]
    fn test_type_list_destroy_failure() {
        let c = C {};
        let config_list = list_value![1u32, 2u32, (), 4u32, 5u32, ()];
        let mut failing = TestTypeListFailingDestroy::create(config_list, &mut &c).unwrap();
        assert!(matches!(
            failing.destroy(&mut &c),
            Err(ConsDestroyError::Tail(ConsDestroyError::Tail(
                ConsDestroyError::Head(_)
            )))
        ));
    }
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

pub trait Subset<L: TypeList, T: Marker>: TypeList {
    // type SubsetRef<'a>;
    // type SubsetMut<'a>;

    fn sub_get<'a>(superset: &'a L) -> Self::RefList<'a>;

    /// # Safety
    /// If subset lists each element uniquely, then
    /// it is safe to obtain mutable references to the superset contained items by reborrowing
    /// the superset mutable reference. Otherwise, multiple mutable references to the same
    /// element may be obtained, which is not allowed and may cause undefined behavior due to aliasing mutable references.
    ///
    /// User must ensure that the subset list does not contain duplicate elements.
    unsafe fn sub_get_mut<'a>(superset: &'a mut L) -> Self::MutList<'a>;

    fn sub_write(self, superset: &mut L);
}

impl<L: TypeList, M: Marker> Subset<L, M> for Nil
where
    L: Contains<Nil, M>,
{
    fn sub_get<'a>(superset: &'a L) -> Self::RefList<'a> {
        superset.get()
    }

    unsafe fn sub_get_mut<'a>(superset: &'a mut L) -> Self::MutList<'a> {
        superset.get_mut()
    }

    // No need to write anything to the superset for an empty subset
    fn sub_write(self, _superset: &mut L) {}
}

/// Implements `Subset` for a type-level list, where `Cons<T, N>` is a subset of superset `L`
/// using marker types `M1` and `M2`. This allows extracting references to the subset elements
/// from the superset, ensuring that each subset element corresponds to a unique marker in the superset.
impl<T: 'static, L: TypeList, M1: Marker, M2: Marker, N: Subset<L, M2>> Subset<L, Cons<M1, M2>>
    for Cons<T, N>
where
    L: Contains<T, M1>,
{
    // type SubsetRef<'a> = Cons<&'a T, N::SubsetRef<'a>>;
    // type SubsetMut<'a> = Cons<&'a mut T, N::SubsetMut<'a>>;

    #[inline]
    fn sub_get<'a>(superset: &'a L) -> Self::RefList<'a> {
        Cons::new(superset.get(), N::sub_get(superset))
    }

    #[inline]
    unsafe fn sub_get_mut<'a>(superset: &'a mut L) -> Self::MutList<'a> {
        let mut reborrow = NonNull::new_unchecked(superset);
        Cons::new(superset.get_mut(), N::sub_get_mut(reborrow.as_mut()))
    }

    #[inline]
    fn sub_write(self, superset: &mut L) {
        let Cons { head, tail } = self;
        *superset.get_mut() = head;
        tail.sub_write(superset);
    }
}

// This trait could act as a markr trait, the methods as currently defined
// are tehnically valid, but are not usefull in practice as the type inference
// will not be able to infer the correct type for `L` and `T`.
// Its use requires to explicity state te superet type in the method invocation,
// making the syntax more complex and less ergonomic than directly using Subset methods
// Temporary left here for future reference and potential improvements.
pub trait Superset<L: TypeList, T: Marker>: TypeList {
    fn super_get<'a>(&'a self) -> L::RefList<'a>;
    unsafe fn super_get_mut<'a>(&'a mut self) -> L::MutList<'a>;
}

impl<T: TypeList, L: TypeList, M: Marker> Superset<L, M> for T
where
    L: Subset<T, M>,
{
    fn super_get<'a>(&'a self) -> L::RefList<'a> {
        L::sub_get(self)
    }
    unsafe fn super_get_mut<'a>(&'a mut self) -> L::MutList<'a> {
        L::sub_get_mut(self)
    }
}

#[cfg(test)]
mod test_subset {
    use crate::{Cons, Marker, Nil, Subset, Superset, TypeList};

    type SupersetList = list_type![u16, u32, u64, String, Nil];
    type SubsetList = list_type![u32, String, u16, Nil];

    fn should_compile_subset<M: Marker, L: TypeList + 'static, S: Subset<L, M>>() -> bool {
        true
    }

    fn should_compile_superset<M: Marker, L: TypeList, S: Superset<L, M>>() -> bool {
        true
    }

    #[test]
    fn test_subset_inferred() {
        should_compile_subset::<_, SupersetList, SubsetList>();
        should_compile_superset::<_, SubsetList, SupersetList>();
    }

    #[test]
    fn test_sub_get() {
        let superset: SupersetList = list_value![1u16, 2u32, 3u64, "Hello".to_string(), Nil::new()];
        let unpack_list![a, b, c] = SubsetList::sub_get(&superset);
        assert_eq!(*a, 2u32);
        assert_eq!(*b, "Hello");
        assert_eq!(*c, 1u16);
    }

    #[test]
    fn test_sub_write() {
        let mut superset: SupersetList = list_value![0u16, 0u32, 0u64, String::new(), Nil::new()];
        let subset = list_value![2u32, "Hello".to_string(), 1u16, Nil::new()];
        subset.sub_write(&mut superset);
        let unpack_list![a, b, _64, c] = superset;
        assert_eq!(a, 1u16);
        assert_eq!(c, "Hello");
        assert_eq!(b, 2u32);
    }

    #[test]
    fn test_super_get() {
        let superset: SupersetList = list_value![1u16, 2u32, 3u64, "Hello".to_string(), Nil::new()];
        let subset = <SupersetList as Superset<SubsetList, _>>::super_get(&superset);
        let unpack_list![a, b, c] = subset;
        assert_eq!(*a, 2u32);
        assert_eq!(*b, "Hello");
        assert_eq!(*c, 1u16);
    }

    #[test]
    fn test_sub_get_mut() {
        let mut superset: SupersetList =
            list_value![1u16, 2u32, 3u64, "Hello".to_string(), Nil::new()];
        let unpack_list![a, b, c] = unsafe { SubsetList::sub_get_mut(&mut superset) };
        assert_eq!(*a, 2u32);
        assert_eq!(*b, "Hello");
        assert_eq!(*c, 1u16);
        *a = 42u32;
        *b = "World".to_string();
        *c = 100u16;
        let unpack_list![a, b, _64, c] = superset;
        assert_eq!(a, 100u16);
        assert_eq!(b, 42u32);
        assert_eq!(c, "World");
    }
}
