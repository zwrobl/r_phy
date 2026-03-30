use crate::{
    drop_guard::{
        Create, Destroy,
        test_types::{A, C, FaillingCreate, FaillingDestroy},
    },
    list_type, list_value, unpack_any, unpack_list,
};

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
    let mut superset: SupersetList = list_value![1u16, 2u32, 3u64, "Hello".to_string(), Nil::new()];
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
