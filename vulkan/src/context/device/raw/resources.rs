pub mod buffer;
pub mod image;
pub mod memory;

use std::convert::Infallible;

use buffer::BufferRaw;
use image::{ImageRaw, ImageViewRaw};
use type_kit::{
    list_type, BorrowList, Cons, Contains, Conv, Create, Destroy, DestroyResult, DropGuardError,
    FromGuard, GenCollectionResult, GenIndexRaw, GuardIndex, IndexList, Marked, Marker, Nil,
    ScopedEntryMutResult, ScopedEntryResult, TypeGuard, TypeGuardCollection, TypedIndex, Valid,
};

use crate::context::{
    device::raw::resources::memory::Memory,
    error::{ResourceError, ResourceResult},
    Context,
};

pub trait Resource:
    FromGuard<Inner = Self::RawType>
    + for<'a> Create<Context<'a> = &'a Context, CreateError = ResourceError>
{
    type RawType: Clone + Copy + for<'a> Destroy<Context<'a> = Self::Context<'a>>;
}

pub type Raw<R> = <R as Resource>::RawType;

#[derive(Debug, Clone, Copy)]
pub struct ResourceIndex<R: Resource> {
    index: GuardIndex<R>,
}

impl<R: Resource> FromGuard for ResourceIndex<R> {
    type Inner = GenIndexRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self.index.into_inner()
    }
}

impl<R: Resource> From<Valid<ResourceIndex<R>>> for ResourceIndex<R> {
    fn from(value: Valid<ResourceIndex<R>>) -> Self {
        let index = unsafe { TypeGuard::from_inner::<GuardIndex<R>>(value.into_inner()) };
        let index: Conv<GuardIndex<R>> = index.try_into().unwrap();
        Self {
            index: index.unwrap(),
        }
    }
}

pub type RawCollection<R> = TypeGuardCollection<<R as Resource>::RawType>;
pub type ResourceStorageList = list_type![
    TypeGuardCollection<Memory>,
    TypeGuardCollection<BufferRaw>,
    TypeGuardCollection<ImageRaw>,
    TypeGuardCollection<ImageViewRaw>,
    Nil
];

#[derive(Debug)]
pub struct ResourceStorage {
    storage: ResourceStorageList,
}

impl ResourceStorage {
    #[inline]
    pub fn new() -> Self {
        ResourceStorage {
            storage: ResourceStorageList::default(),
        }
    }

    #[inline]
    pub fn create_resource<'a, R: Resource, M: Marker>(
        &mut self,
        context: &Context,
        config: R::Config<'a>,
    ) -> ResourceResult<ResourceIndex<R>>
    where
        ResourceStorageList: Contains<RawCollection<R>, M>,
    {
        let resource = R::create(config, context)?;
        let index = self.storage.get_mut().push(resource.into_guard())?;
        Ok(ResourceIndex { index })
    }

    #[inline]
    pub fn destroy_resource<R: Resource, M: Marker>(
        &mut self,
        context: &Context,
        index: ResourceIndex<R>,
    ) -> ResourceResult<()>
    where
        ResourceStorageList: Contains<RawCollection<R>, M>,
    {
        let _ = self
            .storage
            .get_mut()
            .pop(index.index)?
            .inner_mut()
            .destroy(context);
        Ok(())
    }

    #[inline]
    pub fn entry<'a, R: Resource, M: Marker>(
        &'a self,
        index: ResourceIndex<R>,
    ) -> ScopedEntryResult<'a, R>
    where
        ResourceStorageList: Contains<RawCollection<R>, M>,
    {
        let ResourceIndex { index } = index;
        self.storage.get().entry(TypedIndex::<R>::new(index))
    }

    #[inline]
    pub fn entry_mut<'a, R: Resource, M: Marker>(
        &'a mut self,
        index: ResourceIndex<R>,
    ) -> ScopedEntryMutResult<'a, R>
    where
        ResourceStorageList: Contains<RawCollection<R>, M>,
    {
        let ResourceIndex { index } = index;
        self.storage
            .get_mut()
            .entry_mut(TypedIndex::<R>::new(index))
    }

    #[inline]
    pub fn opperate_ref<
        I: ResourceIndexList,
        R,
        E,
        F: FnOnce(&<I::List as IndexList<ResourceStorageList>>::Borrowed) -> Result<R, E>,
    >(
        &mut self,
        index: I,
        f: F,
    ) -> GenCollectionResult<Result<R, E>> {
        let index_list = index.into_index_list();
        let borrowed = index_list.get_borrowed(&mut self.storage)?;
        let result = f(&borrowed);
        borrowed.put_back(&mut self.storage)?;
        Ok(result)
    }

    #[inline]
    pub fn opperate_mut<
        I: ResourceIndexList,
        R,
        E,
        F: FnOnce(&mut <I::List as IndexList<ResourceStorageList>>::Borrowed) -> Result<R, E>,
    >(
        &mut self,
        index: I,
        f: F,
    ) -> GenCollectionResult<Result<R, E>> {
        let index_list = index.into_index_list();
        let mut borrowed = index_list.get_borrowed(&mut self.storage)?;
        let result = f(&mut borrowed);
        borrowed.put_back(&mut self.storage)?;
        Ok(result)
    }

    #[inline]
    fn destroy_resource_storage<R: 'static, M: Marker>(
        &mut self,
        context: &Context,
    ) -> DestroyResult<R>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context>,
        ResourceStorageList: Contains<TypeGuardCollection<R>, M>,
    {
        self.storage.get_mut().destroy(context)
    }
}

impl Destroy for ResourceStorage {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.destroy_resource_storage::<ImageViewRaw, _>(context)?;
        self.destroy_resource_storage::<ImageRaw, _>(context)?;
        self.destroy_resource_storage::<BufferRaw, _>(context)?;
        self.destroy_resource_storage::<Memory, _>(context)?;
        Ok(())
    }
}

pub trait ResourceIndexList {
    type List: IndexList<ResourceStorageList>;

    fn into_index_list(self) -> Self::List;
}

impl ResourceIndexList for Nil {
    type List = Nil;

    #[inline]
    fn into_index_list(self) -> Self::List {
        self
    }
}

impl<R: Resource, M: Marker, N: ResourceIndexList> ResourceIndexList
    for Cons<Marked<ResourceIndex<R>, M>, N>
where
    ResourceStorageList: Contains<TypeGuardCollection<R::RawType>, M>,
{
    type List = Cons<Marked<TypedIndex<R>, M>, N::List>;

    #[inline]
    fn into_index_list(self) -> Self::List {
        let Cons {
            head:
                Marked {
                    value: ResourceIndex { index },
                    ..
                },
            tail,
        } = self;

        Cons {
            head: Marked::new(TypedIndex::new(index)),
            tail: tail.into_index_list(),
        }
    }
}
