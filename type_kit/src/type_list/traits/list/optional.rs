use crate::{Cons, TypedNil};

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
