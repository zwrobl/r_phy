use std::{
    any::type_name,
    fmt::{Debug, Formatter},
    marker::PhantomData,
    ops::{Index, IndexMut},
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

#[cfg(test)]
mod test_static_heap_allocator {
    use super::*;

    struct A {
        value: u64,
    }

    struct B {
        value: [u8; 32],
    }

    fn reset_global_state() {
        STATIC_ALLOCATOR_COUNT.store(0, Ordering::Relaxed);
    }

    #[test]
    fn succesfull_allocation() {
        reset_global_state();
        let mut allocator = StaticHeapAllocator::new();
        let index_a1 = allocator.allocate(A { value: 42 });
        let index_b = allocator.allocate(B { value: [0; 32] });
        let index_a2 = allocator.allocate(A { value: 21 });
        assert_eq!(allocator[index_a1].value, 42);
        assert_eq!(allocator[index_a2].value, 21);
        assert!(allocator[index_b].value.iter().all(|&value| value == 0));
        allocator.free(index_a1);
        allocator.free(index_a2);
        allocator.free(index_b);
        assert!(allocator.get_ref(index_a1).is_none());
        assert!(allocator.get_ref(index_a2).is_none());
        assert!(allocator.get_ref(index_b).is_none());
    }

    #[test]
    fn test_accessors() {
        reset_global_state();
        let mut allocator = StaticHeapAllocator::new();
        let index_a = allocator.allocate(A { value: 42 });
        assert!(allocator.get_ref(index_a).is_some_and(|a| a.value == 42));
        allocator.get_mut(index_a).map(|a| a.value = 31);
        assert!(allocator.get_ref(index_a).is_some_and(|a| a.value == 31));
        allocator.free(index_a);
        assert!(allocator.get_ref(index_a).is_none());
        assert!(allocator.get_mut(index_a).is_none());
    }

    #[test]
    #[should_panic(expected = "StaticAllocator: Not all allocations freed prior to allocator drop")]
    fn panic_on_non_empty_drop() {
        reset_global_state();
        let mut allocator = StaticHeapAllocator::new();
        let _ = allocator.allocate(A { value: 42 });
    }

    #[test]
    #[should_panic(expected = "Invalid allocator index for static allocator: expected 1 was 0")]
    fn panic_on_invalid_allocator_index() {
        reset_global_state();
        let mut allocator_1 = StaticHeapAllocator::new();
        let allocator_2 = StaticHeapAllocator::new();
        assert_eq!(allocator_1.index, 0);
        assert_eq!(allocator_2.index, 1);
        let index_a = allocator_1.allocate(A { value: 42 });
        allocator_1.free(index_a);
        let _ = allocator_2[index_a];
    }

    #[test]
    #[should_panic(expected = "Invalid allocator index for static allocator: item dropped")]
    fn panic_on_access_after_free() {
        reset_global_state();
        let mut allocator_1 = StaticHeapAllocator::new();
        let index_a = allocator_1.allocate(A { value: 42 });
        allocator_1.free(index_a);
        let _ = allocator_1[index_a];
    }

    #[test]
    #[should_panic(expected = "StaticHeapAllocator unique indices overflow!")]
    fn panic_on_allocator_index_overflow() {
        reset_global_state();
        STATIC_ALLOCATOR_COUNT.store(usize::MAX, Ordering::Relaxed);
        let _ = StaticHeapAllocator::new();
    }
}

static STATIC_ALLOCATOR_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cold]
#[inline]
fn allocator_count_overflow() {
    panic!("StaticHeapAllocator unique indices overflow!");
}

#[inline]
fn get_next_allocator_index() -> usize {
    loop {
        let index = STATIC_ALLOCATOR_COUNT.load(Ordering::Relaxed);
        if index != usize::MAX {
            if STATIC_ALLOCATOR_COUNT
                .compare_exchange(index, index + 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break index;
            }
        } else {
            allocator_count_overflow();
        }
    }
}

#[derive(Debug)]
pub struct StaticHeapAllocator {
    index: usize,
    allocations: Vec<Option<NonNull<u8>>>,
}

pub struct StaticAllocationIndex<T: Sized> {
    allocator_index: usize,
    item_index: usize,
    _phantom: PhantomData<T>,
}

impl<T: Sized> Debug for StaticAllocationIndex<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("allocator_index", &self.allocator_index)
            .field("item_index", &self.item_index)
            .field("_phantom", &type_name::<T>())
            .finish()
    }
}

impl<T: Sized> Clone for StaticAllocationIndex<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Sized> Copy for StaticAllocationIndex<T> {}

impl Default for StaticHeapAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticHeapAllocator {
    pub fn new() -> Self {
        Self {
            index: get_next_allocator_index(),
            allocations: vec![],
        }
    }

    pub fn allocate<T: Sized>(&mut self, value: T) -> StaticAllocationIndex<T> {
        let allocation = unsafe { NonNull::new_unchecked(Box::leak(Box::new(value)) as *mut T) };
        self.allocations.push(Some(allocation.cast::<u8>()));
        StaticAllocationIndex {
            allocator_index: self.index,
            item_index: self.allocations.len() - 1,
            _phantom: PhantomData,
        }
    }

    pub fn get_ref<T: Sized>(&self, index: StaticAllocationIndex<T>) -> Option<&T> {
        if self.index == index.allocator_index {
            self.allocations
                .get(index.item_index)
                .and_then(|&ptr| ptr.map(|alloc| unsafe { alloc.cast::<T>().as_ref() }))
        } else {
            None
        }
    }

    pub fn get_mut<T: Sized>(&mut self, index: StaticAllocationIndex<T>) -> Option<&mut T> {
        if self.index == index.allocator_index {
            self.allocations
                .get(index.item_index)
                .and_then(|&ptr| ptr.map(|alloc| unsafe { alloc.cast::<T>().as_mut() }))
        } else {
            None
        }
    }

    pub fn free<T: Sized>(&mut self, index: StaticAllocationIndex<T>) {
        if self.index == index.allocator_index {
            self.allocations.get_mut(index.item_index).map(|ptr| {
                ptr.take()
                    .map(|alloc| drop(unsafe { alloc.cast::<T>().read() }))
            });
        }
    }
}

impl<T: Sized> Index<StaticAllocationIndex<T>> for StaticHeapAllocator {
    type Output = T;

    #[inline]
    fn index(&self, index: StaticAllocationIndex<T>) -> &Self::Output {
        if self.index == index.allocator_index {
            if let Some(alloc) = self.allocations[index.item_index] {
                unsafe { alloc.cast::<T>().as_ref() }
            } else {
                panic!("Invalid allocator index for static allocator: item dropped");
            }
        } else {
            panic!(
                "Invalid allocator index for static allocator: expected {} was {}",
                self.index, index.allocator_index
            );
        }
    }
}

impl<T> IndexMut<StaticAllocationIndex<T>> for StaticHeapAllocator {
    #[inline]
    fn index_mut(&mut self, index: StaticAllocationIndex<T>) -> &mut Self::Output {
        if self.index == index.allocator_index {
            if let Some(alloc) = self.allocations[index.item_index] {
                unsafe { alloc.cast::<T>().as_mut() }
            } else {
                panic!("Invalid allocator index for static allocator: item dropped");
            }
        } else {
            panic!(
                "Invalid allocator index for static allocator: expected {} was {}",
                self.index, index.allocator_index
            );
        }
    }
}

impl Drop for StaticHeapAllocator {
    fn drop(&mut self) {
        if self.allocations.iter().any(|&alloc| alloc.is_some()) {
            panic!("StaticAllocator: Not all allocations freed prior to allocator drop");
        }
    }
}
