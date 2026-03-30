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
