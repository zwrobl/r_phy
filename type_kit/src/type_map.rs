use std::{any::TypeId, collections::HashMap};

use crate::{CollectionDestroyError, Create, Destroy, FromGuard};

#[derive(Debug)]
pub struct TypeMap<T> {
    data: HashMap<TypeId, T>,
}

impl<T> Default for TypeMap<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> TypeMap<T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

impl<T> TypeMap<T> {
    #[inline]
    pub fn remove<I: FromGuard<Inner = T>>(&mut self) -> Option<I> {
        self.data
            .remove(&TypeId::of::<I>())
            .map(|inner| unsafe { I::from_inner(inner) })
    }

    #[inline]
    pub fn create<'a, 'b, I: Create<Config<'a> = ()> + FromGuard<Inner = T>>(
        &mut self,
        context: I::Context<'b>,
    ) -> Result<(), I::CreateError> {
        let item = I::create((), context)?.into_inner();
        self.data.insert(TypeId::of::<I>(), item);
        Ok(())
    }

    #[inline]
    pub fn insert<I: FromGuard<Inner = T>>(&mut self, item: I) {
        self.data.insert(TypeId::of::<I>(), item.into_inner());
    }
}

impl<T: Copy + Clone> TypeMap<T> {
    #[inline]
    pub fn get<I: FromGuard<Inner = T>>(&self) -> Option<I> {
        self.data
            .get(&TypeId::of::<I>())
            .map(|&inner| unsafe { I::from_inner(inner) })
    }
}

impl<T: Destroy> Destroy for TypeMap<T>
where
    for<'a> T::Context<'a>: Copy,
{
    type Context<'a> = T::Context<'a>;

    type DestroyError = CollectionDestroyError<T>;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> crate::DestroyResult<Self> {
        let keys: Vec<_> = self.data.keys().copied().collect();
        keys.iter().try_for_each(|key| {
            let mut item = self.data.remove(key).unwrap();
            item.destroy(context).map_err(|err| (item, err))
        })?;
        Ok(())
    }
}
