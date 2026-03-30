mod marker;
mod nodes;

pub use marker::*;
pub use nodes::*;

use std::ptr::NonNull;

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

#[cfg(test)]
mod test_macro {
    use crate::{Cons, Nil, list_type, list_value, unpack_any};

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

/// Macro for convinient definition type lists types.
/// To allow for specific termination type e.g. `TypedNil<T>` with user defined `T` type,
/// following macro does not append `Nil` automatically.
/// To ensure most of the library functionality is correctly derived for the list type,
/// the last element in the list should be the terminator type. e.g. `Nil`. or `Fin<T>`.
///
/// # Example
/// ```rust
/// # use type_kit::{list_type, Nil, Cons};
/// # use std::any::TypeId;
/// type MacroTypeList = list_type![u8, u16, u32, Nil];
/// type ExplicitTypeList = Cons<u8, Cons<u16, Cons<u32, Nil>>>;
///
/// assert_eq!(TypeId::of::<MacroTypeList>(), TypeId::of::<ExplicitTypeList>());
/// ```
#[macro_export]
macro_rules! list_type {
    [$head:ty, $tail:ty] => {
        Cons<$head, $tail>
    };
    [$head:ty $(, $tail:ty)*] => {
        Cons<$head, list_type![$($tail),*]>
    };
}

/// Macro for convinient construction of type lists values.
/// Same as `list_type!` macro, following macro does not append `Nil` automatically.
///
/// # Example
/// ```rust
/// # use type_kit::{list_value, Nil, Cons};
/// let macro_list = list_value![1u8, 2u16, 3u32, Nil::new()];
/// let explicit_list = Cons::new(1u8, Cons::new(2u16, Cons::new(3u32, Nil::new())));
/// assert_eq!(macro_list, explicit_list);
/// ```
#[macro_export]
macro_rules! list_value {
    [$head:expr, $tail:expr] => {
        Cons::new($head, $tail)
    };
    [$head:expr $(, $tail:expr)*] => {
        Cons::new($head, list_value![$($tail),*])
    };
}

/// Macro for convinient unpacking of type list values into their constituent elements.
///
/// The macro generates pattern matching code for destructuring the type list,
/// binding each element, up to the depth defined by the number of identifiers,
/// to the corresponding identifier. Rest of the type list is discarded.
///
/// If the entire list is to be unpacked,
/// last identifier should correspond to the last owned value in the list,
/// and identifier for the last terminator should be omitted.
///
/// Identifiers are specified as non mutable bindings.
/// To allow for mutable access to the unwapped elements, use `unpack_list_mut!` macro.
///
/// # Example
/// ```rust
/// # use type_kit::{list_value, unpack_list, Cons, Nil};
/// let list = list_value![1u8, 2u16, 3u32, Nil::new()];
/// let unpack_list![value_u8, value_u16, value_u32] = list;
/// assert_eq!(value_u8, 1u8);
/// assert_eq!(value_u16, 2u16);
/// assert_eq!(value_u32, 3u32);
/// ```
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

/// Macro for convinient unpacking of type list values into their constituent elements.
///
/// Work similarly to `unpack_list!` macro, but generates mutable bindings for each element.
///
/// # Example
/// ```rust
/// # use type_kit::{list_value, unpack_list_mut, Cons, Nil, TypeList};
/// let mut list = list_value![1u8, 2u16, 3u32, Nil::new()];
/// let unpack_list_mut![value_u8, value_u16, value_u32] = list;
///
/// value_u8 += 1;
/// value_u16 += 2;
/// value_u32 += 3;
///
/// assert_eq!(value_u8, 2u8);
/// assert_eq!(value_u16, 4u16);
/// assert_eq!(value_u32, 6u32);
/// ```
#[macro_export]
macro_rules! unpack_list_mut {
    [$tail:ident] => {
        Cons {
            head: mut $tail,
            ..
        }
    };
    [$head:ident $(, $tail:ident)*] => {
        Cons {
            head: mut $head,
            tail: unpack_list_mut![$($tail),*]
        }
    };
}

/// Macro for convinient unpacking of type list values into their constituent elements,
///
/// Work similarly to `unpack_list!` macro,
/// instead of matching only `Cons` nodes `head` values and discarding the tail for the last unpacked node,
/// this macro binds the last node to last identifier, allowing to capture the entire tail of the list.
///
/// # Example
/// ```rust
/// # use type_kit::{list_value, unpack_any, Cons, Nil};
/// let list = list_value![1u8, 2u16, 3u32];
/// let unpack_any![value_u8, value_u16, value_u32] = list;
/// assert_eq!(value_u8, 1u8);
/// assert_eq!(value_u16, 2u16);
/// assert_eq!(value_u32, 3u32);
/// ```
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

#[cfg(test)]
mod test_type_list_create_destroy {
    use super::*;
    use crate::drop_guard::test_types::{A, C, FaillingCreate, FaillingDestroy};
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

/// Defines update operation for type-level lists containing `Option<T>` items.
pub trait OptionalList {
    /// Updates `Self` with values from `value`, replacing only the `Some` items.
    fn update(&mut self, value: Self);
}

impl OptionalList for TypedNil<()> {
    #[inline]
    fn update(&mut self, _value: Self) {}
}

impl<C: 'static, N: OptionalList> OptionalList for Cons<Option<C>, N> {
    #[inline]
    fn update(&mut self, value: Self) {
        if let Some(value) = value.head {
            self.head = Some(value);
        }
        self.tail.update(value.tail);
    }
}

/// Marker traits denoting that a type list is non-empty.
/// It contains at least one `Cons` node, or is a `Fin` node.
pub trait NonEmptyList: TypeList {}

impl<C, N: NonEmptyList> NonEmptyList for Cons<C, N> {}

impl<H> NonEmptyList for Fin<H> {}

// TODO: Technically, Cons<Nil, Nil> would be marked as NonEmptyList
impl<T, C> NonEmptyList for Cons<C, TypedNil<T>> {}
