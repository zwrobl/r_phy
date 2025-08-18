use std::any::type_name;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::mem::{ManuallyDrop, MaybeUninit};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::type_guard::test_types::{A, B};

    #[test]
    fn test_push_and_get() {
        let mut collection = GenVec::default();
        let index1 = collection.push("Item 1").unwrap();
        let index2 = collection.push("Item 2").unwrap();

        assert_eq!(collection.get(index1).unwrap(), &"Item 1");
        assert_eq!(collection.get(index2).unwrap(), &"Item 2");
    }

    #[test]
    fn test_get_mut() {
        let mut collection = GenVec::default();
        let index = collection.push("Item 1").unwrap();

        {
            let item = collection.get_mut(index).unwrap();
            *item = "Updated Item 1";
        }

        assert_eq!(collection.get(index).unwrap(), &"Updated Item 1");
    }

    #[test]
    fn test_pop() {
        let mut collection = GenVec::default();
        let index1 = collection.push("Item 1").unwrap();
        let index2 = collection.push("Item 2").unwrap();

        let removed_item = collection.pop(index1).unwrap();
        assert_eq!(removed_item, "Item 1");

        // Verify that the second item is still accessible
        assert_eq!(collection.get(index2).unwrap(), &"Item 2");

        // Attempting to get the removed item should fail
        assert!(collection.get(index1).is_err());
    }

    #[test]
    fn test_pop_last() {
        let mut collection = GenVec::default();
        let index = collection.push("Last Item").unwrap();

        let removed_item = collection.pop(index).unwrap();
        assert_eq!(removed_item, "Last Item");

        // Verify that the collection is now empty
        assert!(collection.get(index).is_err());
    }

    #[test]
    fn test_pop_while_last_borrowed() {
        let mut collection = GenVec::<u8>::default();
        let first_index = collection.push(42u8.into()).unwrap();
        let second_index = collection.push(37u8.into()).unwrap();

        let borrowed_item = collection.borrow(second_index).unwrap();
        assert_eq!(*borrowed_item, 37u8);

        let removed_item = collection.pop(first_index).unwrap();
        assert_eq!(removed_item, 42u8);

        collection.put_back(borrowed_item).unwrap();
    }

    #[test]
    fn test_invalid_index() {
        let collection: GenVec<&str> = GenVec::default();
        let invalid_index = GenIndex::wrap(0, 999); // Invalid index

        assert!(matches!(
            collection.get(invalid_index),
            Err(GenCollectionError::InvalidIndex { .. })
        ));
    }

    #[test]
    fn test_generation_mismatch() {
        let mut collection = GenVec::default();
        let index = collection.push("Item 1").unwrap();

        // Manually create an index with an incorrect generation
        let invalid_index = GenIndex::wrap(index.generation + 1, index.index);

        // Attempting to get or pop with the invalid index should fail
        assert!(matches!(
            collection.get(invalid_index),
            Err(GenCollectionError::InvalidGeneration { .. })
        ));
        assert!(matches!(
            collection.pop(invalid_index),
            Err(GenCollectionError::InvalidGeneration { .. })
        ));
    }

    #[test]
    fn test_generation_item_borrowed() {
        let mut collection = GenVec::default();
        let index = collection.push("Item 1").unwrap();

        // Manually create an index with an incorrect generation
        let _borrowed_item = collection.borrow(index);

        // Attempting to get or pop with the invalid index should fail
        assert!(matches!(
            collection.get(index),
            Err(GenCollectionError::CellBorrowed)
        ));
        assert!(matches!(
            collection.pop(index),
            Err(GenCollectionError::CellBorrowed)
        ));
    }

    #[test]
    fn test_iter() {
        let mut collection = GenVec::default();
        collection.push("Item 1").unwrap();
        collection.push("Item 2").unwrap();

        let items: Vec<_> = (&collection).into_iter().cloned().collect();
        assert_eq!(items, vec!["Item 1", "Item 2"]);
    }

    #[test]
    fn test_iter_mut() {
        let mut collection = GenVec::default();
        collection.push("Item 1").unwrap();
        collection.push("Item 2").unwrap();

        for item in &mut collection {
            *item = "Updated";
        }

        let items: Vec<_> = (&collection).into_iter().cloned().collect();
        assert_eq!(items, vec!["Updated", "Updated"]);
    }

    #[test]
    fn test_into_iter() {
        let mut collection = GenVec::default();
        collection.push("Item 1").unwrap();
        collection.push("Item 2").unwrap();

        let items: Vec<_> = collection.into_iter().collect();
        assert_eq!(items, vec!["Item 1", "Item 2"]);
    }

    #[test]
    fn test_drain() {
        let mut collection = GenVec::default();
        collection.push("Item 1").unwrap();
        collection.push("Item 2").unwrap();

        let items: Vec<_> = collection.drain();
        assert_eq!(items, vec!["Item 1", "Item 2"]);
        assert_eq!(collection.len(), 0);
    }

    #[test]
    fn test_filter_drain() {
        let mut collection = GenVec::default();
        let index_1 = collection.push(11).unwrap();
        let index_2 = collection.push(42).unwrap();
        let index_3 = collection.push(31).unwrap();

        let items: Vec<_> = collection.filter_drain(|item| item % 2 == 0);
        assert_eq!(items, vec![42]);
        assert_eq!(collection.len(), 2);
        assert_eq!(collection.get(index_1).unwrap(), &11);
        assert_eq!(collection.get(index_3).unwrap(), &31);

        assert!(matches!(
            collection.get(index_2),
            Err(GenCollectionError::CellEmpty)
        ));

        collection.push(42).unwrap();
        assert!(matches!(
            collection.get(index_2),
            Err(GenCollectionError::InvalidGeneration {
                actual: 1,
                expected: 0
            })
        ));
    }

    #[test]
    fn test_reuse_freed_cells() {
        let mut collection = GenVec::default();
        let index1 = collection.push("Item 1").unwrap();
        let _index2 = collection.push("Item 2").unwrap();

        // Pop the first item, freeing its cell
        collection.pop(index1).unwrap();

        // Push a new item and check if it reuses the freed cell
        let index3 = collection.push("Item 3").unwrap();

        // The new index should reuse the old index1 position
        assert_eq!(index3.index, index1.index);
        assert_eq!(collection.get(index3).unwrap(), &"Item 3");
    }

    #[test]
    fn test_guard_collection_entry_valid_index() {
        let mut collection = GuardVec::<u32>::default();
        let index_a = collection.push(A(42).into_guard()).unwrap();
        let index_b = collection.push(B(31).into_guard()).unwrap();

        let entry: ScopedEntry<'_, A> = collection.entry(TypedIndex::<A, _>::new(index_a)).unwrap();
        assert_eq!(entry.0, 42);
        let entry: ScopedEntry<'_, B> = collection.entry(TypedIndex::<B, _>::new(index_b)).unwrap();
        assert_eq!(entry.0, 31);
    }

    #[test]
    #[cfg(debug_assertions)]
    fn test_guard_collection_entry_invalid_index_type_checked_in_debug() {
        let mut collection = GuardVec::<u32>::default();
        let index_a = collection.push(A(42).into_guard()).unwrap();
        let index_b = collection.push(B(31).into_guard()).unwrap();

        let entry: ScopedEntryResult<B> = collection.entry(TypedIndex::<B, _>::new(index_a));
        assert!(entry.is_err());
        let entry: ScopedEntryResult<A> = collection.entry(TypedIndex::<A, _>::new(index_b));
        assert!(entry.is_err());
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn test_guard_collection_entry_invalid_index_type_check_skip_in_release() {
        let mut collection = GuardVec::<u32>::default();
        let index_a = collection.push(A(42).into_guard()).unwrap();
        let index_b = collection.push(B(31).into_guard()).unwrap();

        let entry_b_invalid: ScopedEntry<'_, B> =
            collection.entry(TypedIndex::<B>::new(index_a)).unwrap();
        assert_eq!(entry_b_invalid.0, 42);
        let entry_a_invalid: ScopedEntry<'_, A> =
            collection.entry(TypedIndex::<A>::new(index_b)).unwrap();
        assert_eq!(entry_a_invalid.0, 31);
    }

    #[test]
    fn test_guard_collection_mut_entry_update_on_drop() {
        let mut collection = GuardVec::<u32>::default();
        let index = collection.push(A(42).into_guard()).unwrap();

        {
            let mut entry: ScopedEntryMut<'_, A> = collection
                .entry_mut(TypedIndex::<A, _>::new(index))
                .unwrap();
            assert_eq!(entry.0, 42);
            entry.0 = 31;
        }

        {
            let entry: ScopedEntryMut<'_, A> = collection
                .entry_mut(TypedIndex::<A, _>::new(index))
                .unwrap();
            assert_eq!(entry.0, 31);
        }
    }

    #[test]
    fn test_gen_index_as_hash_map_key() {
        let mut collection = GenVec::<u32>::default();
        let index1 = collection.push(42).unwrap();
        let index2 = collection.push(32).unwrap();
        let mut map = std::collections::HashMap::new();
        map.insert(index1, 42);
        map.insert(index2, 32);
        assert_eq!(map.get(&index1), Some(&42));
        assert_eq!(map.get(&index2), Some(&32));
    }

    struct DropCounter {
        count: Rc<Cell<usize>>,
    }

    impl DropCounter {
        fn new() -> Self {
            Self {
                count: Rc::new(Cell::new(1)),
            }
        }

        fn count(&self) -> usize {
            self.count.get()
        }
    }

    impl Clone for DropCounter {
        fn clone(&self) -> Self {
            let count = self.count.clone();
            count.set(count.get() + 1);
            Self { count }
        }
    }

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.count.set(self.count.get() - 1);
        }
    }

    #[test]
    fn test_items_dropped_on_collection_drop() {
        let drop_counter = DropCounter::new();
        let mut collection = GenVec::default();
        collection.push(drop_counter.clone()).unwrap();
        collection.push(drop_counter.clone()).unwrap();
        collection.push(drop_counter.clone()).unwrap();
        assert_eq!(drop_counter.count(), 4);
        drop(collection);
        assert_eq!(drop_counter.count(), 1);
    }

    #[test]
    fn test_items_dropped_on_collection_drop_skip_borrowed() {
        let drop_counter = DropCounter::new();
        let mut collection = GenVec::default();

        let index_1 = collection.push(drop_counter.clone()).unwrap();
        let index_2 = collection.push(drop_counter.clone()).unwrap();
        collection.push(drop_counter.clone()).unwrap();
        assert_eq!(drop_counter.count(), 4);

        let borrowed_item = collection.borrow(index_2).unwrap();
        let popped_item = collection.pop(index_1).unwrap();

        drop(collection);
        assert_eq!(drop_counter.count(), 3);

        drop(popped_item);
        drop(borrowed_item);
        assert_eq!(drop_counter.count(), 1);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GenCollectionError {
    InvalidGeneration { expected: usize, actual: usize },
    InvalidIndex { index: usize, len: usize },
    InvalidItemIndex { index: usize, len: usize },
    CellEmpty,
    CellOccupied,
    CellBorrowed,
    TypeGuard(TypeGuardError),
}

impl Display for GenCollectionError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self {
            GenCollectionError::InvalidGeneration { expected, actual } => {
                write!(
                    f,
                    "Invalid generation: expected {}, actual {}",
                    expected, actual
                )
            }
            GenCollectionError::InvalidIndex { index, len } => {
                write!(f, "Invalid index: index {}, len {}", index, len)
            }
            GenCollectionError::InvalidItemIndex { index, len } => {
                write!(f, "Invalid item index: index {}, len {}", index, len)
            }
            GenCollectionError::CellEmpty => {
                write!(f, "Cell is empty")
            }
            GenCollectionError::CellOccupied => {
                write!(f, "Cell is occupied")
            }
            GenCollectionError::CellBorrowed => {
                write!(f, "Cell is borrowed")
            }
            GenCollectionError::TypeGuard(err) => write!(f, "{}", err),
        }
    }
}

impl Error for GenCollectionError {}

pub type GenCollectionResult<T> = Result<T, GenCollectionError>;

impl From<TypeGuardError> for GenCollectionError {
    #[inline]
    fn from(error: TypeGuardError) -> Self {
        GenCollectionError::TypeGuard(error)
    }
}

mod cell {
    use super::{GenCollectionError, GenCollectionResult};

    #[derive(Debug, Clone, Copy)]
    struct Occupied {
        item_index: usize,
    }

    #[derive(Debug, Clone, Copy)]
    struct Empty {
        next_free: Option<usize>,
    }

    #[derive(Debug)]
    pub(super) struct LockedCell {
        cell: IndexCell,
        generation: usize,
    }

    impl LockedCell {
        #[inline]
        pub(super) fn new(item_index: usize) -> Self {
            Self {
                cell: IndexCell::Occupied(Occupied { item_index }),
                generation: 0,
            }
        }

        #[inline]
        pub(super) fn empty() -> Self {
            Self {
                cell: IndexCell::Empty(Empty { next_free: None }),
                generation: 0,
            }
        }

        #[inline]
        pub(super) fn generation(&self) -> GenCollectionResult<usize> {
            match self.cell {
                IndexCell::Occupied(_) => Ok(self.generation),
                IndexCell::Borrowed(_) => Ok(self.generation),
                IndexCell::Empty(..) => Err(GenCollectionError::CellEmpty),
            }
        }

        #[inline]
        pub(super) fn unlock(&self, generation: usize) -> GenCollectionResult<&IndexCell> {
            let cell_generation = self.generation()?;
            if cell_generation == generation {
                Ok(&self.cell)
            } else {
                Err(GenCollectionError::InvalidGeneration {
                    expected: generation,
                    actual: cell_generation,
                })
            }
        }

        #[inline]
        pub(super) fn unlock_mut(
            &mut self,
            generation: usize,
        ) -> GenCollectionResult<&mut IndexCell> {
            let cell_generation = self.generation()?;
            if cell_generation == generation {
                Ok(&mut self.cell)
            } else {
                Err(GenCollectionError::InvalidGeneration {
                    expected: generation,
                    actual: cell_generation,
                })
            }
        }

        #[inline]
        pub(super) fn unlock_unchecked(&mut self) -> &mut IndexCell {
            &mut self.cell
        }

        #[inline]
        pub(super) fn insert(
            &mut self,
            item_index: usize,
        ) -> GenCollectionResult<(usize, Option<usize>)> {
            match self.cell {
                IndexCell::Empty(Empty { next_free }) => {
                    self.generation += 1;
                    self.cell = IndexCell::Occupied(Occupied { item_index });
                    Ok((self.generation, next_free))
                }
                IndexCell::Occupied(..) => Err(GenCollectionError::CellOccupied),
                IndexCell::Borrowed(..) => Err(GenCollectionError::CellBorrowed),
            }
        }

        #[inline]
        pub(super) fn update_item_index(&mut self, item_index: usize) -> GenCollectionResult<()> {
            match &mut self.cell {
                IndexCell::Occupied(cell) => {
                    cell.item_index = item_index;
                    Ok(())
                }
                IndexCell::Borrowed(cell) => {
                    cell.item_index = item_index;
                    Ok(())
                }
                IndexCell::Empty(..) => Err(GenCollectionError::CellEmpty),
            }
        }

        #[inline]
        pub(super) fn is_occupied(&self) -> bool {
            matches!(self.cell, IndexCell::Occupied(..))
        }
    }

    // TODO: Consider simpler tracking of cell state - is borrow information here useful if we also have ItemState?
    #[allow(private_interfaces)]
    #[derive(Debug, Clone, Copy)]
    pub(super) enum IndexCell {
        Occupied(Occupied),
        Borrowed(Occupied),
        Empty(Empty),
    }

    impl IndexCell {
        #[inline]
        pub(super) fn pop(&mut self, next_free: Option<usize>) -> GenCollectionResult<usize> {
            match *self {
                IndexCell::Occupied(cell) => {
                    *self = IndexCell::Empty(Empty { next_free });
                    Ok(cell.item_index)
                }
                IndexCell::Empty(..) => Err(GenCollectionError::CellEmpty),
                IndexCell::Borrowed(..) => Err(GenCollectionError::CellBorrowed),
            }
        }

        #[inline]
        pub(super) fn borrow(&mut self) -> GenCollectionResult<usize> {
            match *self {
                IndexCell::Occupied(cell) => {
                    *self = IndexCell::Borrowed(cell);
                    Ok(cell.item_index)
                }
                IndexCell::Empty(..) => Err(GenCollectionError::CellEmpty),
                IndexCell::Borrowed(..) => Err(GenCollectionError::CellBorrowed),
            }
        }

        #[inline]
        pub(super) fn put_back(&mut self) -> GenCollectionResult<usize> {
            match *self {
                IndexCell::Borrowed(cell) => {
                    *self = IndexCell::Occupied(cell);
                    Ok(cell.item_index)
                }
                IndexCell::Empty(..) => Err(GenCollectionError::CellEmpty),
                IndexCell::Occupied(..) => Err(GenCollectionError::CellOccupied),
            }
        }

        #[inline]
        pub(super) fn item_index(&self) -> GenCollectionResult<usize> {
            match self {
                IndexCell::Occupied(cell) => Ok(cell.item_index),
                IndexCell::Borrowed(..) => Err(GenCollectionError::CellBorrowed),
                IndexCell::Empty(..) => Err(GenCollectionError::CellEmpty),
            }
        }
    }
}

use cell::{IndexCell, LockedCell};
use std::{
    marker::PhantomData,
    ops::{Index, IndexMut},
};

use crate::{
    BoolList, Cons, Contains, Destroy, DestroyResult, DropGuard, Fin, FromGuard, Guard, IntoOuter,
    Marked, Marker, MutListOpt, Nil, OptList, RefListOpt, TypeGuard, TypeGuardError, TypeList,
    TypedNil, ValidMut, ValidRef,
};

pub struct GenIndex<T, C> {
    index: usize,
    generation: usize,
    _phantom: PhantomData<(T, C)>,
}

pub type GenVecIndex<T> = GenIndex<T, GenVec<T>>;
pub type GenCellIndex<T> = GenIndex<T, GenCell<T>>;

impl<T, C> GenIndex<T, C> {
    #[inline]
    pub fn invalid() -> Self {
        Self {
            index: usize::MAX,
            generation: usize::MAX,
            _phantom: PhantomData,
        }
    }
}

impl<T, C> Clone for GenIndex<T, C> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, C> Copy for GenIndex<T, C> {}

impl<T, C> PartialEq for GenIndex<T, C> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<T, C> Eq for GenIndex<T, C> {}

impl<T, C> Hash for GenIndex<T, C> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

impl<T, C> Debug for GenIndex<T, C> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "GenIndex<{}> {{ index: {}, generation: {} }}",
            type_name::<T>(),
            self.index,
            self.generation
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenIndexRaw {
    index: usize,
    generation: usize,
}

impl<T: 'static, C: 'static> FromGuard for GenIndex<T, C> {
    type Inner = GenIndexRaw;

    #[inline]
    fn into_inner(self) -> GenIndexRaw {
        GenIndexRaw {
            index: self.index,
            generation: self.generation,
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        let GenIndexRaw { index, generation } = inner;
        GenIndex::wrap(generation, index)
    }
}

impl<T, C> GenIndex<T, C> {
    #[inline]
    pub fn wrap(generation: usize, index: usize) -> Self {
        Self {
            index,
            generation,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn mark<L, M: Marker>(self) -> Marked<Self, M>
    where
        L: Contains<C, M>,
    {
        Marked::new(self)
    }
}

#[derive(Debug, Clone, Copy)]
enum ItemState {
    Occupied(usize),
    Borrowed(usize),
}

impl ItemState {
    #[inline]
    fn item_index(&self) -> usize {
        match self {
            ItemState::Occupied(index) | ItemState::Borrowed(index) => *index,
        }
    }

    #[inline]
    fn is_occupied(&self) -> bool {
        matches!(self, ItemState::Occupied(_))
    }

    #[inline]
    fn borrow(&mut self) -> GenCollectionResult<()> {
        if let ItemState::Occupied(index) = self {
            *self = ItemState::Borrowed(*index);
            Ok(())
        } else {
            Err(GenCollectionError::CellBorrowed)
        }
    }

    #[inline]
    fn put_back(&mut self) -> GenCollectionResult<()> {
        if let ItemState::Borrowed(index) = self {
            *self = ItemState::Occupied(*index);
            Ok(())
        } else {
            Err(GenCollectionError::CellOccupied)
        }
    }
}

#[derive(Debug)]
pub struct GenVec<T> {
    items: Vec<MaybeUninit<T>>,
    indices: Vec<LockedCell>,
    mapping: Vec<ItemState>,
    next_free: Option<usize>,
}

impl<T> Default for GenVec<T> {
    #[inline]
    fn default() -> Self {
        Self {
            items: Vec::new(),
            indices: Vec::new(),
            mapping: Vec::new(),
            next_free: None,
        }
    }
}

impl<T> Drop for GenVec<T> {
    #[inline]
    fn drop(&mut self) {
        self.items
            .iter_mut()
            .zip(self.mapping.iter())
            .for_each(|(item, &cell_index)| {
                if cell_index.is_occupied() {
                    unsafe {
                        item.assume_init_drop();
                    }
                }
            });
    }
}

impl<T> GenVec<T> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn drain(&mut self) -> Vec<T> {
        self.filter_drain(|_| true)
    }

    #[inline]
    pub fn filter_drain<P>(&mut self, predicate: P) -> Vec<T>
    where
        P: Fn(&T) -> bool,
    {
        let mut removed = Vec::new();
        let mut i = 0;
        while i < self.items.len() {
            let mut item_removed = false;
            if self.mapping[i].is_occupied()
                && predicate(unsafe { self.items[i].assume_init_ref() })
            {
                let cell_index = self.mapping[i].item_index();
                let next_free = self.next_free.replace(cell_index);
                let _ = self.indices[cell_index].unlock_unchecked().pop(next_free);
                removed.push(unsafe { self.swap_remove(i) });
                item_removed = true;
            }
            if !item_removed {
                i += 1;
            }
        }
        removed
    }

    #[inline]
    fn get_cell_unlocked(&self, index: GenIndex<T, Self>) -> GenCollectionResult<&IndexCell> {
        let len = self.indices.len();
        let GenIndex {
            index, generation, ..
        } = index;
        self.indices
            .get(index)
            .ok_or(GenCollectionError::InvalidIndex { index, len })
            .and_then(|cell| cell.unlock(generation))
    }

    #[inline]
    fn get_cell_mut_unlocked(
        &mut self,
        index: GenIndex<T, Self>,
    ) -> GenCollectionResult<&mut IndexCell> {
        let len = self.indices.len();
        let GenIndex {
            index, generation, ..
        } = index;
        self.indices
            .get_mut(index)
            .ok_or(GenCollectionError::InvalidIndex { index, len })
            .and_then(|cell| cell.unlock_mut(generation))
    }

    /// # Safety
    /// The caller must ensure that the item at the given index is occupied
    #[inline]
    unsafe fn swap_remove(&mut self, item_index: usize) -> T {
        let last_index = self.items.len() - 1;
        if item_index < last_index {
            let cell_index = self.mapping[last_index].item_index();
            self.indices[cell_index]
                .update_item_index(item_index)
                .unwrap();
            self.mapping.swap(item_index, last_index);
            self.items.swap(item_index, last_index);
        }
        self.mapping.pop().unwrap();
        unsafe { self.items.pop().unwrap().assume_init() }
    }
}

#[derive(Debug)]
pub struct Borrowed<T, C> {
    item: T,
    index: GenIndex<T, C>,
}

impl<T, C> Deref for Borrowed<T, C> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<T, C> DerefMut for Borrowed<T, C> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

pub trait GenCollection<T>: 'static + Sized {
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
    fn push(&mut self, item: T) -> GenCollectionResult<GenIndex<T, Self>>;
    fn pop(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<T>;
    fn get(&self, index: GenIndex<T, Self>) -> GenCollectionResult<&T>;
    fn get_mut(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<&mut T>;
    fn borrow(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<Borrowed<T, Self>>;
    fn put_back(&mut self, borrow: Borrowed<T, Self>) -> GenCollectionResult<()>;
}

pub trait GuardCollectionT<T>: GenCollection<TypeGuard<T>> {
    #[inline]
    fn entry<'a, I: FromGuard<Inner = T>>(
        &'a self,
        index: TypedIndex<I, Self>,
    ) -> ScopedEntryResult<'a, I>
    where
        T: Clone + Copy,
    {
        let TypedIndex { index } = index;
        let guard = self.get(index)?;
        guard.try_get_scoped_entry()
    }

    #[inline]
    fn entry_mut<'a, I: FromGuard<Inner = T>>(
        &'a mut self,
        index: TypedIndex<I, Self>,
    ) -> ScopedEntryMutResult<'a, I>
    where
        T: Clone + Copy,
    {
        let TypedIndex { index } = index;
        let guard = self.get_mut(index)?;
        guard.try_get_scoped_entry_mut()
    }
}

impl<T, C: GenCollection<TypeGuard<T>>> GuardCollectionT<T> for C {}

impl<T: 'static> GenCollection<T> for GenVec<T> {
    #[inline]
    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[inline]
    fn len(&self) -> usize {
        self.items.len()
    }

    #[inline]
    fn push(&mut self, item: T) -> GenCollectionResult<GenIndex<T, Self>> {
        let item_index = self.items.len();
        self.items.push(MaybeUninit::new(item));

        let (generation, cell_index) = if let Some(index) = self.next_free {
            let cell = &mut self.indices[index];
            let (generation, next_free) = cell.insert(item_index)?;
            self.next_free = next_free;
            (generation, index)
        } else {
            let index = self.indices.len();
            self.indices.push(LockedCell::new(item_index));
            (0, index)
        };

        self.mapping.push(ItemState::Occupied(cell_index));
        Ok(GenIndex::wrap(generation, cell_index))
    }

    #[inline]
    fn pop(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<T> {
        let next_free = self.next_free;
        let item_index = self.get_cell_mut_unlocked(index)?.pop(next_free)?;
        self.next_free.replace(index.index);
        unsafe { Ok(self.swap_remove(item_index)) }
    }

    #[inline]
    fn get(&self, index: GenIndex<T, Self>) -> GenCollectionResult<&T> {
        let item_index = self.get_cell_unlocked(index)?.item_index()?;
        Ok(unsafe { self.items[item_index].assume_init_ref() })
    }

    #[inline]
    fn get_mut(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<&mut T> {
        let item_index = self.get_cell_unlocked(index)?.item_index()?;
        Ok(unsafe { self.items[item_index].assume_init_mut() })
    }

    #[inline]
    fn borrow(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<Borrowed<T, Self>> {
        let item_index = self.get_cell_mut_unlocked(index)?.borrow()?;
        self.mapping[item_index].borrow()?;
        let item = unsafe { self.items[item_index].assume_init_read() };
        Ok(Borrowed { item, index })
    }

    #[inline]
    fn put_back(&mut self, borrow: Borrowed<T, Self>) -> GenCollectionResult<()> {
        let Borrowed { item, index } = borrow;
        let item_index = self.get_cell_mut_unlocked(index)?.put_back()?;
        self.mapping[item_index].put_back()?;
        self.items[item_index] = MaybeUninit::new(item);
        Ok(())
    }
}

impl<T: 'static> Index<GenIndex<T, Self>> for GenVec<T> {
    type Output = T;

    #[inline]
    fn index(&self, index: GenIndex<T, Self>) -> &Self::Output {
        self.get(index).unwrap()
    }
}

impl<T: 'static> IndexMut<GenIndex<T, Self>> for GenVec<T> {
    #[inline]
    fn index_mut(&mut self, index: GenIndex<T, Self>) -> &mut Self::Output {
        self.get_mut(index).unwrap()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GenCollectionRefIter<'a, T> {
    collection: &'a GenVec<T>,
    next: usize,
}

impl<'a, T> Iterator for GenCollectionRefIter<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mapping = &self.collection.mapping;
        let items = &self.collection.items;

        while self.next < items.len() {
            let item_index = self.next;
            self.next += 1;
            if mapping[item_index].is_occupied() {
                return Some(unsafe { items[item_index].assume_init_ref() });
            }
        }
        None
    }
}

impl<'a, T> IntoIterator for &'a GenVec<T> {
    type Item = &'a T;
    type IntoIter = GenCollectionRefIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        GenCollectionRefIter {
            collection: self,
            next: 0,
        }
    }
}

#[derive(Debug)]
pub struct GenCollectionMutIter<'a, T> {
    collection: &'a mut GenVec<T>,
    next: usize,
}

impl<'a, T> Iterator for GenCollectionMutIter<'a, T> {
    type Item = &'a mut T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mapping = &self.collection.mapping;
        let items = &mut self.collection.items;

        while self.next < items.len() {
            let item_index = self.next;
            self.next += 1;
            if mapping[item_index].is_occupied() {
                return Some(unsafe { &mut *items[item_index].as_mut_ptr() });
            }
        }
        None
    }
}

impl<'a, T> IntoIterator for &'a mut GenVec<T> {
    type Item = &'a mut T;
    type IntoIter = GenCollectionMutIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        GenCollectionMutIter {
            collection: self,
            next: 0,
        }
    }
}

#[derive(Debug)]
pub struct GenCollectionIntoIter<T> {
    items: Vec<MaybeUninit<T>>,
    mapping: Vec<ItemState>,
    next: usize,
}

impl<T> Iterator for GenCollectionIntoIter<T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.items.len() {
            let item_indx = self.next;
            self.next += 1;
            if self.mapping[item_indx].is_occupied() {
                return Some(unsafe { self.items[item_indx].assume_init_read() });
            }
        }
        None
    }
}

impl<T: 'static> IntoIterator for GenVec<T> {
    type Item = T;
    type IntoIter = GenCollectionIntoIter<T>;

    #[inline]
    fn into_iter(mut self) -> Self::IntoIter {
        GenCollectionIntoIter {
            items: std::mem::take(&mut self.items),
            mapping: std::mem::take(&mut self.mapping),
            next: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScopedEntry<'a, T: FromGuard> {
    resource: ManuallyDrop<T>,
    _raw: &'a T::Inner,
}

pub type ScopedEntryResult<'a, T> = Result<ScopedEntry<'a, T>, GenCollectionError>;

impl<T: Clone + Copy> TypeGuard<T> {
    #[inline]
    pub fn try_get_scoped_entry<I: FromGuard<Inner = T>>(&self) -> ScopedEntryResult<'_, I> {
        Ok(ScopedEntry {
            resource: ManuallyDrop::new(I::try_from_guard(*self).map_err(|(_, err)| err)?),
            _raw: self.inner(),
        })
    }
}

impl<'a, T: FromGuard> Deref for ScopedEntry<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.resource
    }
}

pub type ScopedEntryMutResult<'a, T> = Result<ScopedEntryMut<'a, T>, GenCollectionError>;

impl<T: Clone + Copy> TypeGuard<T> {
    #[inline]
    pub fn try_get_scoped_entry_mut<I: FromGuard<Inner = T>>(
        &mut self,
    ) -> ScopedEntryMutResult<'_, I> {
        Ok(ScopedEntryMut {
            resource: Some(I::try_from_guard(*self).map_err(|(_, err)| err)?),
            raw: self.inner_mut(),
        })
    }
}

pub struct ScopedEntryMut<'a, T: FromGuard> {
    resource: Option<T>,
    raw: &'a mut T::Inner,
}

impl<'a, T: FromGuard> Drop for ScopedEntryMut<'a, T> {
    #[inline]
    fn drop(&mut self) {
        *self.raw = self.resource.take().unwrap().into_inner();
    }
}

impl<'a, T: FromGuard> Deref for ScopedEntryMut<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.resource.as_ref().unwrap()
    }
}

impl<'a, T: FromGuard> DerefMut for ScopedEntryMut<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.resource.as_mut().unwrap()
    }
}

pub struct ScopedInnerRef<'a, T: FromGuard> {
    inner: &'a T::Inner,
    _phantom: PhantomData<T>,
}

impl<'a, T: FromGuard> From<ValidRef<'a, T>> for ScopedInnerRef<'a, T> {
    #[inline]
    fn from(value: ValidRef<'a, T>) -> Self {
        Self {
            inner: value.inner_ref(),
            _phantom: PhantomData,
        }
    }
}

impl<'a, T: FromGuard> Deref for ScopedInnerRef<'a, T> {
    type Target = T::Inner;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

pub struct ScopedInnerMut<'a, T: FromGuard> {
    inner: &'a mut T::Inner,
    _phantom: PhantomData<T>,
}

impl<'a, T: FromGuard> From<ValidMut<'a, T>> for ScopedInnerMut<'a, T> {
    #[inline]
    fn from(value: ValidMut<'a, T>) -> Self {
        Self {
            inner: value.inner_mut(),
            _phantom: PhantomData,
        }
    }
}

impl<'a, T: FromGuard> Deref for ScopedInnerMut<'a, T> {
    type Target = T::Inner;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'a, T: FromGuard> DerefMut for ScopedInnerMut<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

pub type GuardIndex<T, C> = GenIndex<Guard<T>, C>;
pub type GuardVec<T> = GenVec<TypeGuard<T>>;

#[derive(Debug)]
pub struct TypedIndex<T: FromGuard, C> {
    index: GuardIndex<T, C>,
}

impl<T: FromGuard, C> Clone for TypedIndex<T, C> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: FromGuard, C> Copy for TypedIndex<T, C> {}

impl<T: FromGuard, C> TypedIndex<T, C> {
    #[inline]
    pub fn new(index: GuardIndex<T, C>) -> Self {
        Self { index }
    }

    #[inline]
    pub fn mark<L, M: Marker>(self) -> Marked<Self, M>
    where
        L: Contains<GuardVec<T::Inner>, M>,
    {
        Marked::new(self)
    }
}

impl<I: Clone + Copy + 'static> GuardVec<I> {
    #[inline]
    pub fn entry<'a, T: FromGuard<Inner = I>>(
        &'a self,
        index: TypedIndex<T, Self>,
    ) -> ScopedEntryResult<'a, T> {
        let TypedIndex { index } = index;
        let guard = self.get(index)?;
        guard.try_get_scoped_entry()
    }

    #[inline]
    pub fn entry_mut<'a, T: FromGuard<Inner = I>>(
        &'a mut self,
        index: TypedIndex<T, Self>,
    ) -> ScopedEntryMutResult<'a, T> {
        let TypedIndex { index } = index;
        let guard = self.get_mut(index)?;
        guard.try_get_scoped_entry_mut()
    }
}

pub type ScopedInnerResult<'a, T> = Result<ScopedInnerRef<'a, T>, GenCollectionError>;
pub type ScopedInnerMutResult<'a, T> = Result<ScopedInnerMut<'a, T>, GenCollectionError>;

impl<I: 'static> GuardVec<I> {
    #[inline]
    pub fn inner_ref<'a, T: FromGuard<Inner = I>>(
        &'a self,
        index: GuardIndex<T, Self>,
    ) -> ScopedInnerResult<'a, T> {
        let inner: ValidRef<T> = self.get(index)?.try_into()?;
        Ok(inner.into())
    }

    #[inline]
    pub fn inner_mut<'a, T: FromGuard<Inner = I>>(
        &'a mut self,
        index: GuardIndex<T, Self>,
    ) -> ScopedInnerMutResult<'a, T> {
        let inner: ValidMut<T> = self.get_mut(index)?.try_into()?;
        Ok(inner.into())
    }
}

impl<I: Destroy> Destroy for GenVec<I>
where
    for<'a> I::Context<'a>: Clone + Copy,
{
    type Context<'a> = I::Context<'a>;
    type DestroyError = I::DestroyError;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.into_iter().try_for_each(|item| item.destroy(context))
    }
}

pub trait IndexList<C: 'static>: Sized {
    type Owned;
    type Borrowed: BorrowList<C>;
    type Ref<'a>;

    fn insert(value: Self::Owned, collection: &mut C) -> GenCollectionResult<Self>;
    fn get_ref(self, collection: &C) -> GenCollectionResult<Self::Ref<'_>>;
    fn get_owned(self, collection: &mut C) -> GenCollectionResult<Self::Owned>;
    fn get_borrowed(self, collection: &mut C) -> GenCollectionResult<Self::Borrowed>;
}

impl<C: 'static> IndexList<C> for Nil {
    type Owned = Nil;
    type Borrowed = Nil;
    type Ref<'a> = Nil;

    #[inline]
    fn insert(_: Self::Owned, _: &mut C) -> GenCollectionResult<Self> {
        Ok(Nil::new())
    }

    #[inline]
    fn get_ref(self, _: &C) -> GenCollectionResult<Self::Ref<'_>> {
        Ok(Nil::new())
    }

    #[inline]
    fn get_owned(self, _: &mut C) -> GenCollectionResult<Self::Owned> {
        Ok(Nil::new())
    }

    fn get_borrowed(self, _: &mut C) -> GenCollectionResult<Self::Borrowed> {
        Ok(Nil::new())
    }
}

impl<L: 'static, H: 'static, M: Marker, C: GenCollection<H>, T: IndexList<L>> IndexList<L>
    for Cons<Marked<GenIndex<H, C>, M>, T>
where
    L: Contains<C, M>,
{
    type Owned = Cons<H, T::Owned>;
    type Borrowed = Cons<Marked<Borrowed<H, C>, M>, T::Borrowed>;
    type Ref<'a> = Cons<&'a H, T::Ref<'a>>;

    #[inline]
    fn insert(value: Self::Owned, collection: &mut L) -> GenCollectionResult<Self> {
        let Cons { head, tail } = value;
        let head = Marked::new(collection.get_mut().push(head)?);
        let tail = T::insert(tail, collection)?;
        Ok(Cons::new(head, tail))
    }

    #[inline]
    fn get_ref(self, collection: &L) -> GenCollectionResult<Self::Ref<'_>> {
        let Cons {
            head: Marked { value: index, .. },
            tail,
        } = self;
        let head = collection.get().get(index)?;
        let tail = tail.get_ref(collection)?;
        Ok(Cons::new(head, tail))
    }

    #[inline]
    fn get_owned(self, collection: &mut L) -> GenCollectionResult<Self::Owned> {
        let Cons {
            head: Marked { value: index, .. },
            tail,
        } = self;
        let head = collection.get_mut().pop(index)?;
        let tail = tail.get_owned(collection)?;
        Ok(Cons::new(head, tail))
    }

    #[inline]
    fn get_borrowed(self, collection: &mut L) -> GenCollectionResult<Self::Borrowed> {
        let Cons {
            head: Marked { value: index, .. },
            tail,
        } = self;
        let tail = tail.get_borrowed(collection)?;
        match collection.get_mut().borrow(index) {
            Ok(item) => Ok(Cons::new(Marked::new(item), tail)),
            Err(err) => {
                tail.put_back(collection).unwrap();
                Err(err)
            }
        }
    }
}

impl<L: 'static, H: 'static, M: Marker, C: GenCollection<H>, T: IndexList<L>> IndexList<L>
    for Cons<Option<Marked<GenIndex<H, C>, M>>, T>
where
    L: Contains<C, M>,
{
    type Owned = Cons<Option<H>, T::Owned>;
    type Borrowed = Cons<Option<Marked<Borrowed<H, C>, M>>, T::Borrowed>;
    type Ref<'a> = Cons<Option<&'a H>, T::Ref<'a>>;

    #[inline]
    fn insert(value: Self::Owned, collection: &mut L) -> GenCollectionResult<Self> {
        let Cons { head, tail } = value;
        let head = match head {
            Some(head) => Some(Marked::new(collection.get_mut().push(head)?)),
            None => None,
        };
        let tail = T::insert(tail, collection)?;
        Ok(Cons::new(head, tail))
    }

    #[inline]
    fn get_ref(self, collection: &L) -> GenCollectionResult<Self::Ref<'_>> {
        let Cons { head, tail } = self;
        let item = match head {
            Some(Marked { value: index, .. }) => {
                let item = collection.get().get(index)?;
                Some(item)
            }
            None => None,
        };
        let tail = tail.get_ref(collection)?;
        Ok(Cons::new(item, tail))
    }

    #[inline]
    fn get_owned(self, collection: &mut L) -> GenCollectionResult<Self::Owned> {
        let Cons { head, tail } = self;
        let head = match head {
            Some(Marked { value: index, .. }) => {
                let item = collection.get_mut().pop(index)?;
                Some(item)
            }
            None => None,
        };
        let tail = tail.get_owned(collection)?;
        Ok(Cons::new(head, tail))
    }

    #[inline]
    fn get_borrowed(self, collection: &mut L) -> GenCollectionResult<Self::Borrowed> {
        let Cons { head, tail } = self;
        let tail = tail.get_borrowed(collection)?;
        let item = match head {
            Some(Marked { value: index, .. }) => match collection.get_mut().borrow(index) {
                Ok(item) => Some(Marked::new(item)),
                Err(err) => {
                    tail.put_back(collection).unwrap();
                    return Err(err);
                }
            },
            None => None,
        };
        Ok(Cons::new(item, tail))
    }
}

pub trait BorrowList<C: 'static>: 'static {
    type InnerRef<'a>;
    type InnerMut<'a>;

    fn inner_ref<'a>(&'a self) -> Self::InnerRef<'a>;
    fn inner_mut<'a>(&'a mut self) -> Self::InnerMut<'a>;

    // Consider if here failure to put back the borrowed item should be considered a fatal error, resulting in pacnic
    // This is because if any single item on the list fails to be put back, the entire list must be considered invalid.
    // It is not possible to defined static error type for this case,
    // as the type varies depend on which entries failed to be put back
    // Hence current implementation leaves a possibility for a partial put back, leaving the collection with a borrowed cells
    // which may not be ever returned. This could cause errors in the future access to the collection,
    // or could be handled by allowing to 'prune' the collection from borrowed cells
    fn put_back(self, collection: &mut C) -> GenCollectionResult<()>;
}

impl<C: 'static> BorrowList<C> for Nil {
    type InnerRef<'a> = Self;
    type InnerMut<'a> = Self;

    #[inline]
    fn inner_ref<'a>(&'a self) -> Self::InnerRef<'a> {
        *self
    }

    #[inline]
    fn inner_mut<'a>(&'a mut self) -> Self::InnerMut<'a> {
        *self
    }

    #[inline]
    fn put_back(self, _: &mut C) -> GenCollectionResult<()> {
        Ok(())
    }
}

impl<L: 'static, H: 'static, C: GenCollection<H>, M: Marker, T: BorrowList<L>> BorrowList<L>
    for Cons<Marked<Borrowed<H, C>, M>, T>
where
    L: Contains<C, M>,
{
    type InnerRef<'a> = Cons<&'a H, T::InnerRef<'a>>;

    type InnerMut<'a> = Cons<&'a mut H, T::InnerMut<'a>>;

    #[inline]
    fn inner_ref<'a>(&'a self) -> Self::InnerRef<'a> {
        Cons::new(&self.head.value.item, self.tail.inner_ref())
    }

    #[inline]
    fn inner_mut<'a>(&'a mut self) -> Self::InnerMut<'a> {
        Cons::new(&mut self.head.value.item, self.tail.inner_mut())
    }

    #[inline]
    fn put_back(self, collection: &mut L) -> GenCollectionResult<()> {
        let Cons {
            head: Marked { value: borrow, .. },
            tail,
        } = self;
        collection.get_mut().put_back(borrow)?;
        tail.put_back(collection)
    }
}

impl<L: 'static, H: 'static, C: GenCollection<H>, M: Marker, T: BorrowList<L>> BorrowList<L>
    for Cons<Option<Marked<Borrowed<H, C>, M>>, T>
where
    L: Contains<C, M>,
{
    type InnerRef<'a> = Cons<Option<&'a H>, T::InnerRef<'a>>;

    type InnerMut<'a> = Cons<Option<&'a mut H>, T::InnerMut<'a>>;

    #[inline]
    fn inner_ref<'a>(&'a self) -> Self::InnerRef<'a> {
        Cons::new(
            self.head.as_ref().map(|head| &head.value.item),
            self.tail.inner_ref(),
        )
    }

    #[inline]
    fn inner_mut<'a>(&'a mut self) -> Self::InnerMut<'a> {
        Cons::new(
            self.head.as_mut().map(|head| &mut head.value.item),
            self.tail.inner_mut(),
        )
    }

    #[inline]
    fn put_back(self, collection: &mut L) -> GenCollectionResult<()> {
        let Cons { head, tail } = self;
        if let Some(Marked { value: borrow, .. }) = head {
            collection.get_mut().put_back(borrow)?;
        }
        tail.put_back(collection)
    }
}

#[macro_export]
macro_rules! mark {
    [$collection:ty] => { Nil::new() };
    [$collection:ty, $index:expr $(, $indices:expr)*] => {
        Cons::new($index.mark::<$collection, _>(), mark![$collection $(, $indices)*])
    };
}

pub trait ListIterator {
    type IteratorItem: BoolList;

    fn next(&mut self) -> Self::IteratorItem;
}

impl<T: 'static> ListIterator for TypedNil<T> {
    type IteratorItem = Self;

    #[inline]
    fn next(&mut self) -> Self::IteratorItem {
        *self
    }
}

impl<'a, T: 'static, N: ListIterator> ListIterator for Cons<GenCollectionRefIter<'a, T>, N> {
    type IteratorItem = Cons<Option<&'a T>, N::IteratorItem>;

    #[inline]
    fn next(&mut self) -> Self::IteratorItem {
        let item = <GenCollectionRefIter<_> as Iterator>::next(&mut self.head);
        Cons::new(item, self.tail.next())
    }
}

impl<'a, T: 'static, N: ListIterator> ListIterator for Cons<GenCollectionMutIter<'a, T>, N> {
    type IteratorItem = Cons<Option<&'a mut T>, N::IteratorItem>;

    #[inline]
    fn next(&mut self) -> Self::IteratorItem {
        let item = <GenCollectionMutIter<_> as Iterator>::next(&mut self.head);
        Cons::new(item, self.tail.next())
    }
}

impl<T: 'static, N: ListIterator> ListIterator for Cons<GenCollectionIntoIter<T>, N> {
    type IteratorItem = Cons<Option<T>, N::IteratorItem>;

    #[inline]
    fn next(&mut self) -> Self::IteratorItem {
        let item = <GenCollectionIntoIter<_> as Iterator>::next(&mut self.head);
        Cons::new(item, self.tail.next())
    }
}

impl<'a, T: 'static> ListIterator for GenCollectionRefIter<'a, T> {
    type IteratorItem = Option<&'a T>;

    #[inline]
    fn next(&mut self) -> Self::IteratorItem {
        <GenCollectionRefIter<_> as Iterator>::next(self)
    }
}

impl<'a, T: 'static> ListIterator for GenCollectionMutIter<'a, T> {
    type IteratorItem = Option<&'a mut T>;

    #[inline]
    fn next(&mut self) -> Self::IteratorItem {
        <GenCollectionMutIter<_> as Iterator>::next(self)
    }
}

impl<T: 'static> ListIterator for GenCollectionIntoIter<T> {
    type IteratorItem = Option<T>;

    #[inline]
    fn next(&mut self) -> Self::IteratorItem {
        <GenCollectionIntoIter<_> as Iterator>::next(self)
    }
}

pub struct ListIter<T: ListIterator> {
    iter: T,
}

impl<T: ListIterator> Iterator for ListIter<T> {
    type Item = T::IteratorItem;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next();
        if item.any() { Some(item) } else { None }
    }
}

impl<T: ListIterator> ListIter<T> {
    #[inline]
    pub fn iter_ref<'a, N: IntoCollectionIterator<RefIterator<'a> = T>>(collection: &'a N) -> Self {
        Self {
            iter: collection.iter_ref(),
        }
    }

    #[inline]
    pub fn iter_mut<'a, N: IntoCollectionIterator<MutIterator<'a> = T>>(
        collection: &'a mut N,
    ) -> Self {
        Self {
            iter: collection.iter_mut(),
        }
    }

    #[inline]
    pub fn into_iter<N: IntoCollectionIterator<IntoIterator = T>>(collection: N) -> Self {
        Self {
            iter: collection.into_iter(),
        }
    }

    #[inline]
    pub fn iter_sub<
        'a,
        M: Marker,
        C: TypeList,
        N: IntoSubsetIterator<C, M, RefIterator<'a> = T> + 'a,
    >(
        collection: &'a C,
    ) -> Self {
        Self {
            iter: N::sub_iter(collection),
        }
    }

    /// # Safety
    /// Subset must contain only unique types, as otherwise aliased mutable references to the collection may be created
    #[inline]
    pub unsafe fn iter_sub_mut<
        'a,
        M: Marker,
        C: TypeList,
        N: IntoSubsetIterator<C, M, MutIterator<'a> = T> + 'a,
    >(
        collection: &'a mut C,
    ) -> Self {
        Self {
            iter: unsafe { N::sub_iter_mut(collection) },
        }
    }

    #[inline]
    pub fn all(self) -> ListIterAll<T> {
        ListIterAll { iter: self.iter }
    }
}

pub struct ListIterAll<T: ListIterator> {
    iter: T,
}

impl<T: ListIterator> Iterator for ListIterAll<T> {
    type Item = T::IteratorItem;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next();
        if item.all() { Some(item) } else { None }
    }
}

pub trait IntoCollectionIterator: TypeList + Default + 'static {
    type ItemList: TypeList;
    type RefIterator<'a>: ListIterator<IteratorItem = RefListOpt<'a, Self::ItemList>>
    where
        Self: 'a;
    type MutIterator<'a>: ListIterator<IteratorItem = MutListOpt<'a, Self::ItemList>>
    where
        Self: 'a;
    type IntoIterator: ListIterator<IteratorItem = OptList<Self::ItemList>>;

    fn iter_ref<'a>(&'a self) -> Self::RefIterator<'a>;
    fn iter_mut<'a>(&'a mut self) -> Self::MutIterator<'a>;
    fn into_iter(self) -> Self::IntoIterator;
}

impl IntoCollectionIterator for Nil {
    type ItemList = Nil;
    type RefIterator<'a> = Nil;
    type MutIterator<'a> = Nil;
    type IntoIterator = Nil;

    fn iter_ref<'a>(&'a self) -> Self::RefIterator<'a> {
        *self
    }

    fn iter_mut<'a>(&'a mut self) -> Self::MutIterator<'a> {
        *self
    }

    fn into_iter(self) -> Self::IntoIterator {
        self
    }
}

impl<C: 'static, N: IntoCollectionIterator> IntoCollectionIterator for Cons<GenVec<C>, N> {
    type ItemList = Cons<C, N::ItemList>;
    type RefIterator<'a>
        = Cons<GenCollectionRefIter<'a, C>, N::RefIterator<'a>>
    where
        Self: 'a;
    type MutIterator<'a>
        = Cons<GenCollectionMutIter<'a, C>, N::MutIterator<'a>>
    where
        Self: 'a;
    type IntoIterator = Cons<GenCollectionIntoIter<C>, N::IntoIterator>;

    fn iter_mut<'a>(&'a mut self) -> Self::MutIterator<'a> {
        Cons::new((&mut self.head).into_iter(), self.tail.iter_mut())
    }

    fn iter_ref<'a>(&'a self) -> Self::RefIterator<'a> {
        Cons::new((&self.head).into_iter(), self.tail.iter_ref())
    }

    fn into_iter(self) -> Self::IntoIterator {
        Cons::new(self.head.into_iter(), self.tail.into_iter())
    }
}

#[cfg(test)]
mod test_list_iterator {
    use crate::{list_type, unpack_list};

    use super::*;

    type GenVecCollection = list_type![GenVec<u8>, GenVec<u16>, GenVec<u32>, Nil];

    #[test]
    fn test_ref_list_iterator() {
        let mut collection = GenVecCollection::default();

        let gen_vec_u8 = collection.get_mut::<GenVec<u8>, _>();
        let _ = gen_vec_u8.push(1);
        let _ = gen_vec_u8.push(2);
        let _ = gen_vec_u8.push(3);

        let gen_vec_u16 = collection.get_mut::<GenVec<u16>, _>();
        let _ = gen_vec_u16.push(1);
        let _ = gen_vec_u16.push(2);

        let gen_vec_u32 = collection.get_mut::<GenVec<u32>, _>();
        let _ = gen_vec_u32.push(1);

        let mut iter = ListIter::iter_ref(&collection);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_some());
        assert!(value_u32.is_some());
        assert!(*value_u8.unwrap() == 1);
        assert!(*value_u16.unwrap() == 1);
        assert!(*value_u32.unwrap() == 1);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_some());
        assert!(value_u32.is_none());
        assert!(*value_u8.unwrap() == 2);
        assert!(*value_u16.unwrap() == 2);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_none());
        assert!(value_u32.is_none());
        assert!(*value_u8.unwrap() == 3);

        let next = iter.next();
        assert!(next.is_none());
    }

    #[test]
    fn test_mut_list_iterator() {
        let mut collection = GenVecCollection::default();

        let gen_vec_u8 = collection.get_mut::<GenVec<u8>, _>();
        let _ = gen_vec_u8.push(1);
        let _ = gen_vec_u8.push(2);
        let _ = gen_vec_u8.push(3);

        let gen_vec_u16 = collection.get_mut::<GenVec<u16>, _>();
        let _ = gen_vec_u16.push(1);
        let _ = gen_vec_u16.push(2);

        let gen_vec_u32 = collection.get_mut::<GenVec<u32>, _>();
        let _ = gen_vec_u32.push(1);

        let mut iter = ListIter::iter_mut(&mut collection);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_some());
        assert!(value_u32.is_some());
        *value_u8.unwrap() += 1;
        *value_u16.unwrap() += 1;
        *value_u32.unwrap() += 1;

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_some());
        assert!(value_u32.is_none());
        *value_u8.unwrap() += 1;
        *value_u16.unwrap() += 1;

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_none());
        assert!(value_u32.is_none());
        *value_u8.unwrap() += 1;

        let next = iter.next();
        assert!(next.is_none());

        let mut iter = ListIter::iter_ref(&collection);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_some());
        assert!(value_u32.is_some());
        assert!(*value_u8.unwrap() == 2);
        assert!(*value_u16.unwrap() == 2);
        assert!(*value_u32.unwrap() == 2);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_some());
        assert!(value_u32.is_none());
        assert!(*value_u8.unwrap() == 3);
        assert!(*value_u16.unwrap() == 3);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_none());
        assert!(value_u32.is_none());
        assert!(*value_u8.unwrap() == 4);

        let next = iter.next();
        assert!(next.is_none());
    }

    #[test]
    fn test_into_list_iterator() {
        let mut collection = GenVecCollection::default();

        let gen_vec_u8 = collection.get_mut::<GenVec<u8>, _>();
        let _ = gen_vec_u8.push(1);
        let _ = gen_vec_u8.push(2);
        let _ = gen_vec_u8.push(3);

        let gen_vec_u16 = collection.get_mut::<GenVec<u16>, _>();
        let _ = gen_vec_u16.push(1);
        let _ = gen_vec_u16.push(2);

        let gen_vec_u32 = collection.get_mut::<GenVec<u32>, _>();
        let _ = gen_vec_u32.push(1);

        let mut iter = ListIter::into_iter(collection);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_some());
        assert!(value_u32.is_some());
        assert!(value_u8.unwrap() == 1);
        assert!(value_u16.unwrap() == 1);
        assert!(value_u32.unwrap() == 1);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_some());
        assert!(value_u32.is_none());
        assert!(value_u8.unwrap() == 2);
        assert!(value_u16.unwrap() == 2);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16, value_u32] = next.unwrap();
        assert!(value_u8.is_some());
        assert!(value_u16.is_none());
        assert!(value_u32.is_none());
        assert!(value_u8.unwrap() == 3);

        let next = iter.next();
        assert!(next.is_none());
    }
}

#[derive(Debug)]
pub struct GenCollectionList<T: TypeList + 'static> {
    collection: T,
}

impl<T: TypeList> Deref for GenCollectionList<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.collection
    }
}

impl<T: TypeList> DerefMut for GenCollectionList<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.collection
    }
}

impl<T: TypeList + Default> Default for GenCollectionList<T> {
    #[inline]
    fn default() -> Self {
        Self {
            collection: T::default(),
        }
    }
}

impl<T: TypeList + Default> GenCollectionList<T> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug)]
pub struct BorrowedContext<C: 'static, B: BorrowList<C>> {
    borrow: Option<B>,
    _phantom: PhantomData<C>,
}

impl<C: 'static, B: BorrowList<C>> BorrowedContext<C, B> {
    #[inline]
    pub fn operate_ref<R, F: FnOnce(B::InnerRef<'_>) -> R>(&self, operation: F) -> R {
        operation(self.borrow.as_ref().unwrap().inner_ref())
    }

    #[inline]
    pub fn operate_mut<R, F: FnOnce(B::InnerMut<'_>) -> R>(&mut self, operation: F) -> R {
        operation(self.borrow.as_mut().unwrap().inner_mut())
    }
}

impl<C, B: BorrowList<C>> Destroy for BorrowedContext<C, B> {
    type Context<'a> = &'a mut C;
    type DestroyError = GenCollectionError;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        if let Some(borrow) = self.borrow.take() {
            borrow.put_back(context)?;
            self.borrow = None;
        }
        Ok(())
    }
}

impl<T: TypeList> GenCollectionList<T> {
    #[inline]
    pub fn is_empty<I, C: GenCollection<I>, M: Marker>(&self) -> bool
    where
        T: Contains<C, M>,
    {
        self.collection.get().is_empty()
    }

    #[inline]
    pub fn len<I, C: GenCollection<I>, M: Marker>(&self) -> usize
    where
        T: Contains<C, M>,
    {
        self.collection.get().len()
    }

    #[inline]
    pub fn push<I, C: GenCollection<I>, M: Marker>(
        &mut self,
        item: I,
    ) -> GenCollectionResult<GenIndex<I, C>>
    where
        T: Contains<C, M>,
    {
        self.collection.get_mut().push(item)
    }

    #[inline]
    pub fn pop<I, C: GenCollection<I>, M: Marker>(
        &mut self,
        index: GenIndex<I, C>,
    ) -> GenCollectionResult<I>
    where
        T: Contains<C, M>,
    {
        self.collection.get_mut().pop(index)
    }

    #[inline]
    pub fn insert<I: IndexList<T>>(&mut self, value: I::Owned) -> GenCollectionResult<I> {
        I::insert(value, &mut self.collection)
    }

    #[inline]
    pub fn get_ref<I: IndexList<T>>(&self, index: I) -> GenCollectionResult<I::Ref<'_>> {
        index.get_ref(&self.collection)
    }

    #[inline]
    pub fn get_owned<I: IndexList<T>>(&mut self, index: I) -> GenCollectionResult<I::Owned> {
        index.get_owned(&mut self.collection)
    }

    #[inline]
    pub fn get_borrow<I: IndexList<T>>(
        &mut self,
        index: I,
    ) -> GenCollectionResult<DropGuard<BorrowedContext<T, I::Borrowed>>> {
        let borrow = index.get_borrowed(&mut self.collection)?;
        let context = BorrowedContext {
            borrow: Some(borrow),
            _phantom: PhantomData,
        };
        Ok(DropGuard::new(context))
    }
}

#[cfg(test)]
mod test_list_index {
    use super::*;
    use crate::{Cons, GenIndex, IndexList, Nil, list_type, list_value, unpack_list};

    type TestCopyCollection = list_type![GenVec<u8>, GenVec<u16>, GenVec<u32>, Nil];

    type TestNonCopyCollection = list_type![GenVec<Vec<u8>>, GenVec<Vec<u16>>, Nil];

    type TestCollectionList = GenCollectionList<TestCopyCollection>;

    #[test]
    fn test_collection_list_index_get_owned() {
        let mut collection = TestCopyCollection::default();

        let collection_u8: &mut GenVec<u8> = collection.get_mut();
        let index_u8: GenIndex<u8, _> = collection_u8.push(8).unwrap();

        let collection_u16: &mut GenVec<u16> = collection.get_mut();
        let index_u16: GenIndex<u16, _> = collection_u16.push(16).unwrap();

        let collection_u32: &mut GenVec<u32> = collection.get_mut();
        let index_u32: GenIndex<u32, _> = collection_u32.push(32).unwrap();

        let index_list = mark![TestCopyCollection, index_u8, index_u16, index_u32];
        let unpack_list![b_u8, b_u16, b_u32] = index_list.get_owned(&mut collection).unwrap();

        assert_eq!(b_u8, 8);
        assert_eq!(b_u16, 16);
        assert_eq!(b_u32, 32);

        let collection_u8: &GenVec<u8> = collection.get();
        let collection_u16: &GenVec<u16> = collection.get();
        let collection_u32: &GenVec<u32> = collection.get();

        assert_eq!(collection_u8.len(), 0);
        assert_eq!(collection_u16.len(), 0);
        assert_eq!(collection_u32.len(), 0);
    }

    #[test]
    fn test_collection_list_index_get_ref() {
        let mut collection = TestCopyCollection::default();

        let collection_u8: &mut GenVec<u8> = collection.get_mut();
        let index_u8: GenIndex<u8, _> = collection_u8.push(8).unwrap();

        let collection_u16: &mut GenVec<u16> = collection.get_mut();
        let index_u16: GenIndex<u16, _> = collection_u16.push(16).unwrap();

        let collection_u32: &mut GenVec<u32> = collection.get_mut();
        let index_u32: GenIndex<u32, _> = collection_u32.push(32).unwrap();

        let index_list = mark![TestCopyCollection, index_u8, index_u16, index_u32];
        let unpack_list![b_u8, b_u16, b_u32] = index_list.get_ref(&collection).unwrap();

        assert_eq!(*b_u8, 8);
        assert_eq!(*b_u16, 16);
        assert_eq!(*b_u32, 32);

        let collection_u8: &GenVec<u8> = collection.get();
        let collection_u16: &GenVec<u16> = collection.get();
        let collection_u32: &GenVec<u32> = collection.get();

        assert_eq!(collection_u8.len(), 1);
        assert_eq!(collection_u16.len(), 1);
        assert_eq!(collection_u32.len(), 1);
    }

    #[test]
    fn test_collection_list_index_get_borrow_copy_type() {
        let mut collection = TestCopyCollection::default();

        let collection_u8: &mut GenVec<u8> = collection.get_mut();
        let index_u8: GenIndex<u8, _> = collection_u8.push(8).unwrap();

        let collection_u16: &mut GenVec<u16> = collection.get_mut();
        let index_u16: GenIndex<u16, _> = collection_u16.push(16).unwrap();

        let collection_u32: &mut GenVec<u32> = collection.get_mut();
        let index_u32: GenIndex<u32, _> = collection_u32.push(32).unwrap();

        let index_list = mark![TestCopyCollection, index_u8, index_u16, index_u32];
        let unpack_list![b_u8, b_u16, b_u32] = index_list.get_borrowed(&mut collection).unwrap();

        assert_eq!(**b_u8, 8);
        assert_eq!(**b_u16, 16);
        assert_eq!(**b_u32, 32);

        let collection_u8: &GenVec<u8> = collection.get();
        let collection_u16: &GenVec<u16> = collection.get();
        let collection_u32: &GenVec<u32> = collection.get();

        assert_eq!(collection_u8.len(), 1);
        assert_eq!(collection_u16.len(), 1);
        assert_eq!(collection_u32.len(), 1);

        let collection_u8: &mut GenVec<u8> = collection.get_mut();
        assert!(matches!(
            collection_u8.pop(index_u8),
            Err(GenCollectionError::CellBorrowed)
        ));

        let collection_u16: &mut GenVec<u16> = collection.get_mut();
        assert!(matches!(
            collection_u16.pop(index_u16),
            Err(GenCollectionError::CellBorrowed)
        ));

        let collection_u32: &mut GenVec<u32> = collection.get_mut();
        assert!(matches!(
            collection_u32.pop(index_u32),
            Err(GenCollectionError::CellBorrowed)
        ));

        let borrowed = list_value![b_u8, b_u16, b_u32, Nil::new()];
        assert!(matches!(borrowed.put_back(&mut collection), Ok(..)));

        let collection_u8: &mut GenVec<u8> = collection.get_mut();
        assert!(matches!(collection_u8.pop(index_u8), Ok(8)));

        let collection_u16: &mut GenVec<u16> = collection.get_mut();
        assert!(matches!(collection_u16.pop(index_u16), Ok(16)));

        let collection_u32: &mut GenVec<u32> = collection.get_mut();
        assert!(matches!(collection_u32.pop(index_u32), Ok(32)));
    }

    #[test]
    fn test_collection_list_index_get_borrow_non_copy_type() {
        let mut collection = TestNonCopyCollection::default();

        let collection_vec_u8: &mut GenVec<Vec<u8>> = collection.get_mut();
        let index_vec_u8: GenIndex<Vec<u8>, _> = collection_vec_u8.push(vec![8]).unwrap();

        let collection_vec_u16: &mut GenVec<Vec<u16>> = collection.get_mut();
        let index_vec_u16: GenIndex<Vec<u16>, _> = collection_vec_u16.push(vec![16]).unwrap();

        let index_list = mark![TestNonCopyCollection, index_vec_u8, index_vec_u16];
        let unpack_list![b_vec_u8, b_vec_u16] = index_list.get_borrowed(&mut collection).unwrap();

        assert_eq!(**b_vec_u8, vec![8]);
        assert_eq!(**b_vec_u16, vec![16]);

        let collection_vec_u8: &GenVec<Vec<u8>> = collection.get();
        let collection_vec_u16: &GenVec<Vec<u16>> = collection.get();

        assert_eq!(collection_vec_u8.len(), 1);
        assert_eq!(collection_vec_u16.len(), 1);

        let collection_vec_u8: &mut GenVec<Vec<u8>> = collection.get_mut();
        assert!(matches!(
            collection_vec_u8.pop(index_vec_u8),
            Err(GenCollectionError::CellBorrowed)
        ));

        let collection_vec_u16: &mut GenVec<Vec<u16>> = collection.get_mut();
        assert!(matches!(
            collection_vec_u16.pop(index_vec_u16),
            Err(GenCollectionError::CellBorrowed)
        ));

        let borrowed = list_value![b_vec_u8, b_vec_u16, Nil::new()];
        assert!(matches!(borrowed.put_back(&mut collection), Ok(..)));

        let collection_vec_u8: &mut GenVec<Vec<u8>> = collection.get_mut();
        assert!(matches!(collection_vec_u8.pop(index_vec_u8), Ok(..)));

        let collection_vec_u16: &mut GenVec<Vec<u16>> = collection.get_mut();
        assert!(matches!(collection_vec_u16.pop(index_vec_u16), Ok(..)));
    }

    #[test]
    fn test_gen_collection_list() {
        let mut collection = TestCollectionList::new();
        let index_u8: GenIndex<u8, GenVec<u8>> = collection.push(8u8.into()).unwrap();
        let index_u16: GenIndex<u16, GenVec<u16>> = collection.push(16u16.into()).unwrap();
        let index_u32: GenIndex<u32, GenVec<u32>> = collection.push(32u32.into()).unwrap();

        let index_list = mark![TestCopyCollection, index_u8, index_u16, index_u32];
        {
            let mut context = collection.get_borrow(index_list).unwrap();
            context.operate_ref(|borrow| {
                let unpack_list![b_u8, b_u16, b_u32] = borrow;
                assert_eq!(*b_u8, 8);
                assert_eq!(*b_u16, 16);
                assert_eq!(*b_u32, 32);
            });
            context.operate_mut(|borrow| {
                let unpack_list![b_u8, b_u16, b_u32] = borrow;
                *b_u8 = 7;
                *b_u16 = 15;
                *b_u32 = 31;
            });
            assert!(context.destroy(&mut collection).is_ok());
        }
        {
            let mut context = collection.get_borrow(index_list).unwrap();
            context.operate_ref(|borrow| {
                let unpack_list![b_u8, b_u16, b_u32] = borrow;
                assert_eq!(*b_u8, 7);
                assert_eq!(*b_u16, 15);
                assert_eq!(*b_u32, 31);
            });
            assert!(context.destroy(&mut collection).is_ok());
        }
    }
}

#[derive(Debug)]
pub struct BorrowedGuard<T: FromGuard, C> {
    item: T,
    index: TypedIndex<T, C>,
}

impl<T: FromGuard, C> Deref for BorrowedGuard<T, C> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<T: FromGuard, C> DerefMut for BorrowedGuard<T, C> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

impl<T: FromGuard, C> From<BorrowedGuard<T, C>> for Borrowed<Guard<T>, C> {
    #[inline]
    fn from(value: BorrowedGuard<T, C>) -> Self {
        let BorrowedGuard {
            item,
            index: TypedIndex { index },
        } = value;
        Borrowed {
            item: item.into_guard(),
            index,
        }
    }
}

pub type BorrowGuardError<I, C> = (Borrowed<TypeGuard<I>, C>, TypeGuardError);

impl<T: FromGuard, C> TryFrom<Borrowed<Guard<T>, C>> for BorrowedGuard<T, C> {
    type Error = BorrowGuardError<T::Inner, C>;

    #[inline]
    fn try_from(value: Borrowed<Guard<T>, C>) -> Result<Self, Self::Error> {
        let Borrowed { item, index } = value;
        Ok(Self {
            item: T::try_from_guard(item)
                .map_err(|(guard, err)| (Borrowed { item: guard, index }, err))?,
            index: TypedIndex { index },
        })
    }
}

impl<L: 'static, H: FromGuard, C: GuardCollectionT<H::Inner>, M: Marker, T: BorrowList<L>>
    BorrowList<L> for Cons<Marked<BorrowedGuard<H, C>, M>, T>
where
    L: Contains<C, M>,
{
    type InnerRef<'a> = Cons<&'a H, T::InnerRef<'a>>;
    type InnerMut<'a> = Cons<&'a mut H, T::InnerMut<'a>>;

    #[inline]
    fn inner_ref<'a>(&'a self) -> Self::InnerRef<'a> {
        Cons::new(&self.head.value.item, self.tail.inner_ref())
    }

    #[inline]
    fn inner_mut<'a>(&'a mut self) -> Self::InnerMut<'a> {
        Cons::new(&mut self.head.value.item, self.tail.inner_mut())
    }

    #[inline]
    fn put_back(self, collection: &mut L) -> GenCollectionResult<()> {
        let Cons {
            head: Marked { value: borrow, .. },
            tail,
        } = self;
        collection.get_mut().put_back(borrow.into())?;
        tail.put_back(collection)
    }
}

impl<L: 'static, H: FromGuard, C: GuardCollectionT<H::Inner>, M: Marker, T: IndexList<L>>
    IndexList<L> for Cons<Marked<TypedIndex<H, C>, M>, T>
where
    L: Contains<C, M>,
{
    type Borrowed = Cons<Marked<BorrowedGuard<H, C>, M>, T::Borrowed>;
    type Owned = Cons<H, T::Owned>;
    type Ref<'a> = Cons<&'a TypeGuard<H::Inner>, T::Ref<'a>>;

    #[inline]
    fn insert(value: Self::Owned, collection: &mut L) -> GenCollectionResult<Self> {
        let Cons { head, tail } = value;
        let head = Marked::new(TypedIndex::new(
            collection.get_mut().push(head.into_guard())?,
        ));
        let tail = T::insert(tail, collection)?;
        Ok(Cons::new(head, tail))
    }

    #[inline]
    fn get_ref(self, collection: &L) -> GenCollectionResult<Self::Ref<'_>> {
        let Cons {
            head:
                Marked {
                    value: TypedIndex { index },
                    ..
                },
            tail,
        } = self;
        let head = collection.get().get(index)?;
        let tail = tail.get_ref(collection)?;
        Ok(Cons::new(head, tail))
    }

    // Consider error handling for when the some, other than last, index on the list is invalid
    // This could lead to resources beein leaked, as the collection would not be able to put back the borrowed resources,
    // which in case of resources that should be manually released, e.g. implementing Destroy trait, could lead to memory leaks
    // Hence, for these 'index lists' the error handling should be performed for all indices,
    // before any state is modified, and only then the items should be pulled out of the collections,
    // this way we would always end with the correct state of the collection, either the items properly removed and handed to the user,
    // so the user is responsible for their destruction, or the items are put back to the collection, so the collection can handle their destruction on drop
    #[inline]
    fn get_owned(self, collection: &mut L) -> GenCollectionResult<Self::Owned> {
        let Cons {
            head:
                Marked {
                    value: TypedIndex { index },
                    ..
                },
            tail,
        } = self;
        let tail = tail.get_owned(collection)?;
        let head = collection
            .get_mut()
            .pop(index)?
            .try_into_outer()
            .map_err(|(_, err)| GenCollectionError::TypeGuard(err))?;

        Ok(Cons::new(head, tail))
    }

    #[inline]
    fn get_borrowed(self, collection: &mut L) -> GenCollectionResult<Self::Borrowed> {
        let Cons {
            head:
                Marked {
                    value: TypedIndex { index },
                    ..
                },
            tail,
        } = self;
        let tail = tail.get_borrowed(collection)?;
        let result = match collection.get_mut().borrow(index) {
            Ok(borrow) => match borrow.try_into() {
                Ok(borrow) => Ok(borrow),
                Err((borrow, err)) => {
                    collection.get_mut().put_back(borrow).unwrap();
                    Err(GenCollectionError::TypeGuard(err))
                }
            },
            Err(err) => Err(err),
        };
        match result {
            Ok(borrow) => Ok(Cons::new(Marked::new(borrow), tail)),
            Err(err) => {
                tail.put_back(collection).unwrap();
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod test_type_guard_borrow_list {
    use super::*;

    use crate::{
        Cons, Nil, list_type,
        type_guard::test_types::{A, B},
        unpack_list,
    };

    type TestTypeGuardCollection = list_type![GuardVec<u32>, Nil];
    type TestTypeGuardCollectionList = GenCollectionList<TestTypeGuardCollection>;

    #[test]
    fn test_type_guard_borrow() {
        let mut collection = TestTypeGuardCollection::default();
        let index_a = TypedIndex::<A, _>::new(collection.push(A(42).into_guard()).unwrap());
        let index_b = TypedIndex::<B, _>::new(collection.push(B(42).into_guard()).unwrap());

        let index_list = mark![TestTypeGuardCollection, index_a, index_b];
        let borrow = index_list.get_borrowed(&mut collection).unwrap();
        borrow.put_back(&mut collection).unwrap();
    }

    #[test]
    fn test_invalid_type_cast_does_not_invalidate_collection() {
        let mut collection = TestTypeGuardCollection::default();
        let index_inner_a = collection.push(A(42).into_guard()).unwrap();
        let index_inner_b = collection.push(B(31).into_guard()).unwrap();

        let index_a_invalid = TypedIndex::<A, _>::new(index_inner_b);
        let index_b_invalid = TypedIndex::<B, _>::new(index_inner_a);

        let index_list = mark![TestTypeGuardCollection, index_a_invalid, index_b_invalid];
        let borrow = index_list.get_borrowed(&mut collection);
        assert!(matches!(borrow, Err(GenCollectionError::TypeGuard(..))));

        let index_a_valid = TypedIndex::<A, _>::new(index_inner_a);
        let index_b_valid = TypedIndex::<B, _>::new(index_inner_b);

        let index_list = mark![TestTypeGuardCollection, index_a_valid, index_b_valid];
        let borrow = index_list.get_borrowed(&mut collection);
        assert!(borrow.is_ok());
        borrow.unwrap().put_back(&mut collection).unwrap();
    }

    #[test]
    fn test_type_guard_context_works_with_borrow_context() {
        let mut collection = TestTypeGuardCollectionList::default();
        let index_inner_a = collection.push(A(42).into_guard()).unwrap();
        let index_inner_b = collection.push(B(31).into_guard()).unwrap();

        let index_a = TypedIndex::<A, _>::new(index_inner_a);
        let index_b = TypedIndex::<B, _>::new(index_inner_b);

        let index_list = mark![TestTypeGuardCollection, index_a, index_b];
        {
            let mut borrow = collection.get_borrow(index_list).unwrap();
            let _ = borrow.operate_ref(|borrow| {
                let unpack_list![b_a, b_b] = borrow;
                assert_eq!(b_a.0, 42);
                assert_eq!(b_b.0, 31);
            });
            assert!(borrow.destroy(&mut collection).is_ok());
        }
        {
            let mut borrow = collection.get_borrow(index_list).unwrap();
            let _ = borrow.operate_mut(|borrow| {
                let unpack_list![b_a, b_b] = borrow;
                b_a.0 = 41;
                b_b.0 = 30;
            });
            assert!(borrow.destroy(&mut collection).is_ok());
        }
        {
            let mut borrow = collection.get_borrow(index_list).unwrap();
            let _ = borrow.operate_ref(|borrow| {
                let unpack_list![b_a, b_b] = borrow;
                assert_eq!(b_a.0, 41);
                assert_eq!(b_b.0, 30);
            });
            assert!(borrow.destroy(&mut collection).is_ok());
        }
        let collection_u32: &GuardVec<u32> = collection.get();
        assert_eq!(collection_u32.len(), 2);
    }
}

#[derive(Debug)]
pub struct GenCell<T> {
    cell: LockedCell,
    item: MaybeUninit<T>,
}

pub type GuardCell<T> = GenCell<TypeGuard<T>>;

impl<T: 'static> Default for GenCell<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> GenCell<T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            cell: LockedCell::empty(),
            item: MaybeUninit::uninit(),
        }
    }

    #[inline]
    pub fn replace(&mut self, item: T) -> (GenIndex<T, Self>, Option<T>) {
        // TODO: Here in failing case LockedCell couls possibly be in Borrowed state,
        // currently GuardCell doesen't allow to borrow inner item, thus following cose is correct
        // in the assuption that it is safe to unwrap() try_insert() result
        // in case if GuardCell in future should support borrowing its inner item,
        // handling LockedCell in Borrowed state should be considered here
        let old = self
            .cell
            .unlock_unchecked()
            .pop(None)
            .ok()
            .map(|_| unsafe { self.item.assume_init_read() });
        let index = self.push(item).unwrap();
        (index, old)
    }

    #[inline]
    pub fn drain(&mut self) -> Option<T> {
        if self.cell.unlock_unchecked().pop(None).is_ok() {
            Some(unsafe { self.item.assume_init_read() })
        } else {
            None
        }
    }

    #[inline]
    fn try_unlock(&self, index: GenIndex<T, Self>) -> GenCollectionResult<&IndexCell> {
        match index {
            GenIndex {
                index: 0,
                generation,
                ..
            } => self.cell.unlock(generation),
            GenIndex { index, .. } => Err(GenCollectionError::InvalidIndex { index, len: 1 }),
        }
    }

    #[inline]
    fn try_unlock_mut(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<&mut IndexCell> {
        match index {
            GenIndex {
                index: 0,
                generation,
                ..
            } => self.cell.unlock_mut(generation),
            GenIndex { index, .. } => Err(GenCollectionError::InvalidIndex { index, len: 1 }),
        }
    }
}

impl<T: Clone + Copy + 'static> GenCell<TypeGuard<T>> {
    #[inline]
    pub fn entry<I: FromGuard<Inner = T>>(
        &self,
        index: GuardIndex<I, Self>,
    ) -> ScopedEntryResult<'_, I> {
        self.get(index)?.try_get_scoped_entry()
    }

    #[inline]
    pub fn entry_mut<I: FromGuard<Inner = T>>(
        &mut self,
        index: GuardIndex<I, Self>,
    ) -> ScopedEntryMutResult<'_, I> {
        self.get_mut(index)?.try_get_scoped_entry_mut()
    }
}

impl<T: 'static> GenCollection<T> for GenCell<T> {
    #[inline]
    fn is_empty(&self) -> bool {
        !self.cell.is_occupied()
    }

    #[inline]
    fn len(&self) -> usize {
        if self.cell.is_occupied() { 1 } else { 0 }
    }

    #[inline]
    fn push(&mut self, item: T) -> GenCollectionResult<GenIndex<T, Self>> {
        let (generation, _) = self.cell.insert(0)?;
        self.item = MaybeUninit::new(item);
        Ok(GenIndex::wrap(generation, 0))
    }

    #[inline]
    fn pop(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<T> {
        let _ = self.try_unlock_mut(index)?.pop(None)?;
        Ok(unsafe { self.item.assume_init_read() })
    }

    #[inline]
    fn get(&self, index: GenIndex<T, Self>) -> GenCollectionResult<&T> {
        let _ = self.try_unlock(index)?.item_index()?;
        Ok(unsafe { self.item.assume_init_ref() })
    }

    #[inline]
    fn get_mut(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<&mut T> {
        let _ = self.try_unlock(index)?.item_index()?;
        Ok(unsafe { self.item.assume_init_mut() })
    }

    #[inline]
    fn borrow(&mut self, index: GenIndex<T, Self>) -> GenCollectionResult<Borrowed<T, Self>> {
        let _ = self.try_unlock_mut(index)?.borrow()?;
        Ok(Borrowed {
            item: unsafe { self.item.assume_init_read() },
            index,
        })
    }

    // TODO: This is quite lazy implementation, what if index does not match?
    // Err type should contain original Borrow then to allow external code
    // to properly handle its resource dealllcation e.g. if implement Destory
    #[inline]
    fn put_back(&mut self, borrow: Borrowed<T, Self>) -> GenCollectionResult<()> {
        let Borrowed { item, index } = borrow;
        let _ = self.try_unlock_mut(index)?.put_back()?;
        self.item = MaybeUninit::new(item);
        Ok(())
    }
}

impl<T: Destroy> Destroy for GenCell<T> {
    type Context<'a> = T::Context<'a>;

    type DestroyError = T::DestroyError;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        if self.cell.unlock_unchecked().pop(None).is_ok() {
            unsafe { self.item.assume_init_mut() }.destroy(context)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod test_mixed_collection_types {
    use super::*;

    use crate::{Cons, Nil, list_type, unpack_list};

    type TestCollectionListType = list_type![GenVec<u32>, GenCell<u32>, Nil];
    type TestCollectionList = GenCollectionList<TestCollectionListType>;

    #[test]
    fn test_index_get_owned() {
        let mut collection = TestCollectionListType::default();

        let collection_u32: &mut GenVec<u32> = collection.get_mut();
        let index_collection: GenIndex<u32, _> = collection_u32.push(8).unwrap();

        let cell_u32: &mut GenCell<u32> = collection.get_mut();
        let index_cell: GenIndex<u32, _> = cell_u32.push(16).unwrap();

        let index_list = mark![TestCollectionListType, index_cell, index_collection];
        let unpack_list![item_cell, item_collection] =
            index_list.get_owned(&mut collection).unwrap();

        assert_eq!(item_collection, 8);
        assert_eq!(item_cell, 16);

        let collection_u32: &GenVec<u32> = collection.get();
        let cell_u32: &GenCell<u32> = collection.get();

        assert_eq!(collection_u32.len(), 0);
        assert_eq!(cell_u32.len(), 0);
    }

    #[test]
    fn test_get_borrow() {
        let mut collection = TestCollectionList::default();
        let index_a: GenIndex<u32, GenVec<u32>> = collection.push(42).unwrap();
        let index_b: GenIndex<u32, GenCell<u32>> = collection.push(42).unwrap();

        let index_list = mark![TestCollectionListType, index_a, index_b];
        {
            let mut context = collection.get_borrow(index_list).unwrap();
            context.operate_ref(|unpack_list![item_a, item_b]| {
                assert_eq!(*item_a, 42);
                assert_eq!(*item_b, 42);
            });
            assert!(context.destroy(&mut collection).is_ok());
        }
        {
            let mut context = collection.get_borrow(index_list).unwrap();
            context.operate_mut(|unpack_list![item_a, item_b]| {
                *item_a = 31;
                *item_b = 40;
            });
            assert!(context.destroy(&mut collection).is_ok());
        }
        {
            let mut context = collection.get_borrow(index_list).unwrap();
            context.operate_ref(|unpack_list![item_a, item_b]| {
                assert_eq!(*item_a, 31);
                assert_eq!(*item_b, 40);
            });
            assert!(context.destroy(&mut collection).is_ok());
        }
    }
}

pub struct CollectionType<T: 'static, C: GenCollection<T>> {
    value: T,
    _collection: PhantomData<C>,
}

impl<T: 'static, C: GenCollection<T>> CollectionType<T, C> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self {
            value,
            _collection: PhantomData,
        }
    }
}

pub trait MarkedItemList<C: 'static, M: Marker>: 'static {
    type IndexList: MarkedIndexList<C, M>;

    fn insert(self, collection: &mut C) -> GenCollectionResult<Self::IndexList>;
    fn write<'a>(self, value: <Self::IndexList as MarkedIndexList<C, M>>::Mut<'a>);
}

impl<T: 'static, L: 'static, M: Marker> MarkedItemList<L, M> for TypedNil<T>
where
    L: Contains<TypedNil<T>, M>,
{
    type IndexList = TypedNil<T>;

    #[inline]
    fn insert(self, _collection: &mut L) -> GenCollectionResult<Self::IndexList> {
        Ok(TypedNil::new())
    }

    #[inline]
    fn write<'a>(self, _value: <Self::IndexList as MarkedIndexList<L, M>>::Mut<'a>) {}
}

impl<T: 'static, C: GenCollection<T>, L: 'static, M1: Marker, M2: Marker, N: MarkedItemList<L, M2>>
    MarkedItemList<L, Cons<M1, M2>> for Cons<CollectionType<T, C>, N>
where
    L: Contains<C, M1>,
{
    type IndexList = Cons<GenIndex<T, C>, N::IndexList>;

    #[inline]
    fn insert(self, collection: &mut L) -> GenCollectionResult<Self::IndexList> {
        let Cons { head, tail } = self;
        let head = collection.get_mut().push(head.value)?;
        let tail = tail.insert(collection)?;
        Ok(Cons::new(head, tail))
    }

    #[inline]
    fn write<'a>(self, value: <Self::IndexList as MarkedIndexList<L, Cons<M1, M2>>>::Mut<'a>) {
        let Cons { head, tail } = self;
        *value.head = head.value;
        tail.write(value.tail);
    }
}

impl<T: 'static, C: GenCollection<T>, L: 'static, M1: Marker, M2: Marker, N: MarkedItemList<L, M2>>
    MarkedItemList<L, Cons<M1, M2>> for Cons<Option<CollectionType<T, C>>, N>
where
    L: Contains<C, M1>,
{
    type IndexList = Cons<Option<GenIndex<T, C>>, N::IndexList>;

    #[inline]
    fn insert(self, collection: &mut L) -> GenCollectionResult<Self::IndexList> {
        let Cons { head, tail } = self;
        let head = match head {
            Some(item) => Some(collection.get_mut().push(item.value)?),
            None => None,
        };
        let tail = tail.insert(collection)?;
        Ok(Cons::new(head, tail))
    }

    #[inline]
    fn write<'a>(self, value: <Self::IndexList as MarkedIndexList<L, Cons<M1, M2>>>::Mut<'a>) {
        let Cons { head, tail } = self;
        if let (Some(head), Some(value_head)) = (head, value.head) {
            *value_head = head.value;
        }
        tail.write(value.tail);
    }
}

pub trait MarkedBorrowList<C: 'static, M: Marker>: 'static {
    type InnerRef<'a>;
    type InnerMut<'a>;

    fn inner_ref<'a>(&'a self) -> Self::InnerRef<'a>;
    fn inner_mut<'a>(&'a mut self) -> Self::InnerMut<'a>;
    fn put_back(self, collection: &mut C) -> GenCollectionResult<()>;
}

impl<L: 'static, T: 'static, M: Marker> MarkedBorrowList<L, M> for TypedNil<T>
where
    L: Contains<TypedNil<T>, M>,
{
    type InnerRef<'a> = &'a Self;
    type InnerMut<'a> = &'a mut Self;

    #[inline]
    fn inner_ref<'a>(&'a self) -> Self::InnerRef<'a> {
        self
    }

    #[inline]
    fn inner_mut<'a>(&'a mut self) -> Self::InnerMut<'a> {
        self
    }

    #[inline]
    fn put_back(self, _collection: &mut L) -> GenCollectionResult<()> {
        Ok(())
    }
}

impl<
    T: 'static,
    C: GenCollection<T>,
    L: 'static,
    M1: Marker,
    M2: Marker,
    N: MarkedBorrowList<L, M2>,
> MarkedBorrowList<L, Cons<M1, M2>> for Cons<Borrowed<T, C>, N>
where
    L: Contains<C, M1>,
{
    type InnerRef<'a> = Cons<&'a T, N::InnerRef<'a>>;
    type InnerMut<'a> = Cons<&'a mut T, N::InnerMut<'a>>;

    #[inline]
    fn inner_ref<'a>(&'a self) -> Self::InnerRef<'a> {
        Cons::new(&self.head.item, self.tail.inner_ref())
    }

    #[inline]
    fn inner_mut<'a>(&'a mut self) -> Self::InnerMut<'a> {
        Cons::new(&mut self.head.item, self.tail.inner_mut())
    }

    #[inline]
    fn put_back(self, collection: &mut L) -> GenCollectionResult<()> {
        collection.get_mut().put_back(self.head)?;
        self.tail.put_back(collection)
    }
}

impl<
    T: 'static,
    C: GenCollection<T>,
    L: 'static,
    M1: Marker,
    M2: Marker,
    N: MarkedBorrowList<L, M2>,
> MarkedBorrowList<L, Cons<M1, M2>> for Cons<Option<Borrowed<T, C>>, N>
where
    L: Contains<C, M1>,
{
    type InnerRef<'a> = Cons<Option<&'a T>, N::InnerRef<'a>>;
    type InnerMut<'a> = Cons<Option<&'a mut T>, N::InnerMut<'a>>;

    #[inline]
    fn inner_ref<'a>(&'a self) -> Self::InnerRef<'a> {
        Cons::new(
            self.head.as_ref().map(|item| item as &T),
            self.tail.inner_ref(),
        )
    }

    #[inline]
    fn inner_mut<'a>(&'a mut self) -> Self::InnerMut<'a> {
        Cons::new(
            self.head.as_mut().map(|item| item as &mut T),
            self.tail.inner_mut(),
        )
    }

    #[inline]
    fn put_back(self, collection: &mut L) -> GenCollectionResult<()> {
        let Cons { head, tail } = self;
        if let Some(borrow) = head {
            collection.get_mut().put_back(borrow)?;
        }
        tail.put_back(collection)
    }
}

pub trait MarkedIndexList<C: 'static, M: Marker>: Sized {
    type Owned: TypeList;
    type Borrowed: MarkedBorrowList<C, M>;
    type Ref<'a>;
    type Mut<'a>;

    fn get_ref(self, collection: &C) -> GenCollectionResult<Self::Ref<'_>>;
    /// # Safety
    /// The caller must ensure that the list index contains only unique elements
    /// Otherwise mutable aliased references would be created
    unsafe fn get_mut(self, collection: &mut C) -> GenCollectionResult<Self::Mut<'_>>;
    fn get_owned(self, collection: &mut C) -> GenCollectionResult<Self::Owned>;
    fn get_borrowed(self, collection: &mut C) -> GenCollectionResult<Self::Borrowed>;
}

impl<L: 'static, T: 'static, M: Marker> MarkedIndexList<L, M> for TypedNil<T>
where
    L: Contains<TypedNil<T>, M>,
{
    type Owned = Self;
    type Borrowed = Self;
    type Ref<'a> = Self;
    type Mut<'a> = Self;

    #[inline]
    fn get_ref(self, _: &L) -> GenCollectionResult<Self::Ref<'_>> {
        Ok(TypedNil::new())
    }

    #[inline]
    unsafe fn get_mut(self, _: &mut L) -> GenCollectionResult<Self::Mut<'_>> {
        Ok(TypedNil::new())
    }

    #[inline]
    fn get_owned(self, _: &mut L) -> GenCollectionResult<Self::Owned> {
        Ok(TypedNil::new())
    }

    #[inline]
    fn get_borrowed(self, _: &mut L) -> GenCollectionResult<Self::Borrowed> {
        Ok(TypedNil::new())
    }
}

impl<T: 'static, C: GenCollection<T>, L: 'static, M1: Marker, M2: Marker, N: MarkedIndexList<L, M2>>
    MarkedIndexList<L, Cons<M1, M2>> for Cons<GenIndex<T, C>, N>
where
    L: Contains<C, M1>,
{
    type Owned = Cons<T, N::Owned>;
    type Borrowed = Cons<Borrowed<T, C>, N::Borrowed>;
    type Ref<'a> = Cons<&'a T, N::Ref<'a>>;
    type Mut<'a> = Cons<&'a mut T, N::Mut<'a>>;

    #[inline]
    fn get_ref(self, collection: &L) -> GenCollectionResult<Self::Ref<'_>> {
        let Cons { head, tail } = self;
        let head = collection.get().get(head)?;
        let tail = tail.get_ref(collection)?;
        Ok(Cons { head, tail })
    }

    #[inline]
    unsafe fn get_mut(self, collection: &mut L) -> GenCollectionResult<Self::Mut<'_>> {
        let Cons { head, tail } = self;
        let mut reborrow = unsafe { NonNull::new_unchecked(collection) };
        let head = collection.get_mut().get_mut(head)?;
        let tail = unsafe { tail.get_mut(reborrow.as_mut())? };
        Ok(Cons { head, tail })
    }

    #[inline]
    fn get_owned(self, collection: &mut L) -> GenCollectionResult<Self::Owned> {
        let Cons { head, tail } = self;
        let head = collection.get_mut().pop(head)?;
        let tail = tail.get_owned(collection)?;
        Ok(Cons { head, tail })
    }

    #[inline]
    fn get_borrowed(self, collection: &mut L) -> GenCollectionResult<Self::Borrowed> {
        let Cons { head, tail } = self;
        let head = collection.get_mut().borrow(head)?;
        let tail = tail.get_borrowed(collection)?;
        Ok(Cons { head, tail })
    }
}

impl<T: 'static, C: GenCollection<T>, L: 'static, M1: Marker, M2: Marker, N: MarkedIndexList<L, M2>>
    MarkedIndexList<L, Cons<M1, M2>> for Cons<Option<GenIndex<T, C>>, N>
where
    L: Contains<C, M1>,
{
    type Owned = Cons<Option<T>, N::Owned>;
    type Borrowed = Cons<Option<Borrowed<T, C>>, N::Borrowed>;
    type Ref<'a> = Cons<Option<&'a T>, N::Ref<'a>>;
    type Mut<'a> = Cons<Option<&'a mut T>, N::Mut<'a>>;

    #[inline]
    fn get_ref(self, collection: &L) -> GenCollectionResult<Self::Ref<'_>> {
        let Cons { head, tail } = self;
        let head = match head {
            Some(index) => Some(collection.get().get(index)?),
            None => None,
        };
        let tail = tail.get_ref(collection)?;
        Ok(Cons { head, tail })
    }

    #[inline]
    unsafe fn get_mut(self, collection: &mut L) -> GenCollectionResult<Self::Mut<'_>> {
        let Cons { head, tail } = self;
        let mut reborrow = unsafe { NonNull::new_unchecked(collection) };
        let head = match head {
            Some(index) => Some(collection.get_mut().get_mut(index)?),
            None => None,
        };
        let tail = unsafe { tail.get_mut(reborrow.as_mut())? };
        Ok(Cons { head, tail })
    }

    #[inline]
    fn get_owned(self, collection: &mut L) -> GenCollectionResult<Self::Owned> {
        let Cons { head, tail } = self;
        let head = match head {
            Some(index) => Some(collection.get_mut().pop(index)?),
            None => None,
        };
        let tail = tail.get_owned(collection)?;
        Ok(Cons { head, tail })
    }

    #[inline]
    fn get_borrowed(self, collection: &mut L) -> GenCollectionResult<Self::Borrowed> {
        let Cons { head, tail } = self;
        let head = match head {
            Some(index) => Some(collection.get_mut().borrow(index)?),
            None => None,
        };
        let tail = tail.get_borrowed(collection)?;
        Ok(Cons { head, tail })
    }
}

#[cfg(test)]
mod test_marked_borrow {
    use crate::{
        CollectionType, Cons, GenCollection, GenIndex, GenVec, MarkedBorrowList, MarkedIndexList,
        MarkedItemList, Nil, list_type, list_value, unpack_list, unpack_list_mut,
    };

    type StorageList = list_type![GenVec<u32>, GenVec<u16>, GenVec<String>, Nil];

    #[test]
    fn test_get_ref() {
        let mut collection = StorageList::default();

        let index_u32: GenIndex<u32, _> = collection.get_mut::<GenVec<u32>, _>().push(42).unwrap();
        let index_u16: GenIndex<u16, _> = collection.get_mut::<GenVec<u16>, _>().push(7).unwrap();
        let index_string: GenIndex<String, _> = collection
            .get_mut::<GenVec<String>, _>()
            .push("Hello".to_string())
            .unwrap();

        let index_list = list_value![index_u32, index_u16, index_string, Nil::new()];

        let index_ref = index_list.get_ref(&collection);

        assert!(index_ref.is_ok());
        let unpack_list![index_ref_u32, index_ref_u16, index_ref_string] = index_ref.unwrap();
        assert_eq!(index_ref_u32, &42);
        assert_eq!(index_ref_u16, &7);
        assert_eq!(index_ref_string, "Hello");
    }

    #[test]
    fn test_get_ref_optional() {
        let mut collection = StorageList::default();

        let index_u32: GenIndex<u32, _> = collection.get_mut::<GenVec<u32>, _>().push(42).unwrap();
        let _index_u16: GenIndex<u16, _> = collection.get_mut::<GenVec<u16>, _>().push(7).unwrap();
        let index_string: GenIndex<String, _> = collection
            .get_mut::<GenVec<String>, _>()
            .push("Hello".to_string())
            .unwrap();

        let index_list = list_value![
            index_u32,
            Option::<GenIndex<_, GenVec<u16>>>::None,
            index_string,
            Nil::new()
        ];

        let index_ref = index_list.get_ref(&collection);

        assert!(index_ref.is_ok());
        let unpack_list![index_ref_u32, index_ref_u16, index_ref_string] = index_ref.unwrap();
        assert_eq!(index_ref_u32, &42);
        assert!(index_ref_u16.is_none());
        assert_eq!(index_ref_string, "Hello");
    }

    #[test]
    fn test_get_borrow() {
        let mut collection = StorageList::default();

        let index_u32: GenIndex<u32, _> = collection.get_mut::<GenVec<u32>, _>().push(42).unwrap();
        let index_u16: GenIndex<u16, _> = collection.get_mut::<GenVec<u16>, _>().push(7).unwrap();
        let index_string: GenIndex<String, _> = collection
            .get_mut::<GenVec<String>, _>()
            .push("Hello".to_string())
            .unwrap();

        let index_list = list_value![index_u32, index_u16, index_string, Nil::new()];

        let borrow_list = index_list.get_borrowed(&mut collection);

        assert!(borrow_list.is_ok());
        let unpack_list_mut![borrow_u32, borrow_u16, borrow_string] = borrow_list.unwrap();

        *borrow_u32 = 21;
        *borrow_u16 = 13;
        *borrow_string = "World".to_string();

        let borrow_list = list_value![borrow_u32, borrow_u16, borrow_string, Nil::new()];
        borrow_list.put_back(&mut collection).unwrap();

        let index_ref = index_list.get_ref(&collection);

        assert!(index_ref.is_ok());
        let unpack_list![index_ref_u32, index_ref_u16, index_ref_string] = index_ref.unwrap();
        assert_eq!(index_ref_u32, &21);
        assert_eq!(index_ref_u16, &13);
        assert_eq!(index_ref_string, "World");
    }

    #[test]
    fn test_insert() {
        let mut collection = StorageList::default();

        let index_list = list_value![
            CollectionType::<_, GenVec<_>>::new(42u32),
            CollectionType::<_, GenVec<_>>::new(7u16),
            CollectionType::<_, GenVec<_>>::new("Hello".to_string()),
            Nil::new()
        ]
        .insert(&mut collection);

        assert!(index_list.is_ok());
        let index_list = index_list.unwrap();
        let index_ref = index_list.get_ref(&mut collection);

        assert!(index_ref.is_ok());
        let unpack_list![index_ref_u32, index_ref_u16, index_ref_string] = index_ref.unwrap();
        assert_eq!(index_ref_u32, &42);
        assert_eq!(index_ref_u16, &7);
        assert_eq!(index_ref_string, "Hello");
    }
}

pub trait IntoSubsetIterator<T: TypeList, M: Marker>: TypeList {
    type RefIterator<'a>: ListIterator<IteratorItem = Self::RefListOpt<'a>>
    where
        T: 'a,
        Self: 'a;
    type MutIterator<'a>: ListIterator<IteratorItem = Self::MutListOpt<'a>>
    where
        T: 'a,
        Self: 'a;

    fn sub_iter<'a>(collection: &'a T) -> Self::RefIterator<'a>;

    /// # Safety
    /// If subset lists each element uniquely, then
    /// it is safe to obtain mutable references to the superset contained items by reborrowing
    /// the superset mutable reference. Otherwise, multiple mutable references to the same
    /// element may be obtained, which is not allowed and may cause undefined behavior due to aliasing mutable references.
    ///
    /// User must ensure that the subset list does not contain duplicate elements.
    unsafe fn sub_iter_mut<'a>(collection: &'a mut T) -> Self::MutIterator<'a>;
}

impl<T: 'static, M: Marker, L: TypeList> IntoSubsetIterator<L, M> for TypedNil<T>
where
    L: Contains<TypedNil<T>, M>,
{
    type RefIterator<'a>
        = TypedNil<T>
    where
        L: 'a,
        Self: 'a;
    type MutIterator<'a>
        = TypedNil<T>
    where
        L: 'a,
        Self: 'a;

    #[inline]
    fn sub_iter<'a>(_collection: &'a L) -> Self::RefIterator<'a> {
        TypedNil::new()
    }

    #[inline]
    unsafe fn sub_iter_mut<'a>(_collection: &'a mut L) -> Self::MutIterator<'a> {
        TypedNil::new()
    }
}

impl<C: 'static, T: TypeList, M1: Marker, M2: Marker, N: IntoSubsetIterator<T, M2>>
    IntoSubsetIterator<T, Cons<M1, M2>> for Cons<C, N>
where
    T: Contains<GenVec<C>, M1>,
{
    type RefIterator<'a>
        = Cons<GenCollectionRefIter<'a, C>, N::RefIterator<'a>>
    where
        T: 'a,
        Self: 'a;

    type MutIterator<'a>
        = Cons<GenCollectionMutIter<'a, C>, N::MutIterator<'a>>
    where
        T: 'a,
        Self: 'a;

    fn sub_iter<'a>(collection: &'a T) -> Self::RefIterator<'a> {
        Cons::new(collection.get().into_iter(), N::sub_iter(collection))
    }

    unsafe fn sub_iter_mut<'a>(collection: &'a mut T) -> Self::MutIterator<'a> {
        let mut reborrow = unsafe { NonNull::new_unchecked(collection) };
        Cons::new(collection.get_mut().into_iter(), unsafe {
            N::sub_iter_mut(reborrow.as_mut())
        })
    }
}

impl<C: 'static, T: TypeList, M: Marker> IntoSubsetIterator<T, M> for Fin<C>
where
    T: Contains<GenVec<C>, M>,
{
    type RefIterator<'a>
        = GenCollectionRefIter<'a, C>
    where
        T: 'a,
        Self: 'a;

    type MutIterator<'a>
        = GenCollectionMutIter<'a, C>
    where
        T: 'a,
        Self: 'a;

    fn sub_iter<'a>(collection: &'a T) -> Self::RefIterator<'a> {
        collection.get().into_iter()
    }

    unsafe fn sub_iter_mut<'a>(collection: &'a mut T) -> Self::MutIterator<'a> {
        collection.get_mut().into_iter()
    }
}

#[cfg(test)]
mod test_subset_iterator {
    use crate::{list_type, unpack_list};

    use super::*;

    type GenVecCollection = list_type![GenVec<u8>, GenVec<u16>, GenVec<u32>, Nil];
    type Subset = list_type![u8, u16, Nil];

    #[test]
    fn test_subset_iterator() {
        let mut collection = GenVecCollection::default();

        let gen_vec_u8 = collection.get_mut::<GenVec<u8>, _>();
        let _ = gen_vec_u8.push(1);
        let _ = gen_vec_u8.push(2);
        let _ = gen_vec_u8.push(3);

        let gen_vec_u16 = collection.get_mut::<GenVec<u16>, _>();
        let _ = gen_vec_u16.push(1);
        let _ = gen_vec_u16.push(2);

        let gen_vec_u32 = collection.get_mut::<GenVec<u32>, _>();
        let _ = gen_vec_u32.push(1);

        let mut iter = ListIter::iter_sub::<_, _, Subset>(&collection).all();

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16] = Subset::unwrap_ref(next.unwrap());
        assert!(*value_u8 == 1);
        assert!(*value_u16 == 1);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16] = Subset::unwrap_ref(next.unwrap());
        assert!(*value_u8 == 2);
        assert!(*value_u16 == 2);

        let next = iter.next();
        assert!(next.is_none());
    }

    #[test]
    fn test_subset_mut_iterator() {
        let mut collection = GenVecCollection::default();

        let gen_vec_u8 = collection.get_mut::<GenVec<u8>, _>();
        let _ = gen_vec_u8.push(1);
        let _ = gen_vec_u8.push(2);
        let _ = gen_vec_u8.push(3);

        let gen_vec_u16 = collection.get_mut::<GenVec<u16>, _>();
        let _ = gen_vec_u16.push(1);
        let _ = gen_vec_u16.push(2);

        let gen_vec_u32 = collection.get_mut::<GenVec<u32>, _>();
        let _ = gen_vec_u32.push(1);

        let mut iter = unsafe { ListIter::iter_sub_mut::<_, _, Subset>(&mut collection).all() };

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16] = Subset::unwrap_mut(next.unwrap());
        *value_u8 += 1;
        *value_u16 += 1;

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16] = Subset::unwrap_mut(next.unwrap());
        *value_u8 += 1;
        *value_u16 += 1;

        let next = iter.next();
        assert!(next.is_none());

        let mut iter = ListIter::iter_sub::<_, _, Subset>(&collection).all();

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16] = Subset::unwrap_ref(next.unwrap());
        assert!(*value_u8 == 2);
        assert!(*value_u16 == 2);

        let next = iter.next();
        assert!(next.is_some());
        let unpack_list![value_u8, value_u16] = Subset::unwrap_ref(next.unwrap());
        assert!(*value_u8 == 3);
        assert!(*value_u16 == 3);

        let next = iter.next();
        assert!(next.is_none());
    }
}
