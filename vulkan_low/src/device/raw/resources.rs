pub mod buffer;
pub mod command;
pub mod descriptor;
pub mod framebuffer;
pub mod image;
pub mod layout;
pub mod memory;
pub mod pipeline;
pub mod render_pass;
pub mod swapchain;

use std::{cell::RefCell, convert::Infallible, fmt::Debug};

use buffer::BufferRaw;
use command::{PersistentCommandPoolRaw, TransientCommandPoolRaw};
use descriptor::DescriptorPoolDataRaw;
use framebuffer::FramebufferRaw;
use image::{ImageRaw, TextureRaw};
use layout::{DescriptorSetLayoutRaw, PipelineLayoutRaw};
use pipeline::GraphicsPipelineRaw;
use type_kit::{
    list_type, BorrowList, CollectionDestroyError, Cons, Contains, Create, Destroy, DestroyResult,
    DropGuardError, FromGuard, GenCollectionResult, GenIndex, GenIndexRaw, GuardIndex, IndexList,
    Marked, Marker, Nil, ScopedEntryMutResult, ScopedEntryResult, TypeGuard, TypeGuardCollection,
    TypeMap, TypedIndex,
};

use crate::{
    device::raw::resources::{memory::MemoryRaw, render_pass::RenderPassRaw},
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

pub struct ResourceIndex<R: Resource> {
    index: GuardIndex<R>,
}

impl<R: Resource> ResourceIndex<R> {
    #[inline]
    pub(crate) fn wrap(index: GuardIndex<R>) -> Self {
        Self { index }
    }

    #[inline]
    pub fn unwrap(self) -> GuardIndex<R> {
        self.index
    }
}

pub type RawIndex = GenIndexRaw;

impl<R: Resource> Clone for ResourceIndex<R> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Resource> Copy for ResourceIndex<R> {}

impl<R: Resource> Debug for ResourceIndex<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceIndex")
            .field("index", &self.index)
            .finish()
    }
}

impl<R: Resource> FromGuard for ResourceIndex<R> {
    type Inner = GenIndexRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self.index.into_inner()
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            index: GuardIndex::<R>::from_inner(inner),
        }
    }
}

pub type RawCollection<R> = TypeGuardCollection<<R as Resource>::RawType>;
pub type ResourceStorageList = list_type![
    TypeGuardCollection<MemoryRaw>,
    TypeGuardCollection<BufferRaw>,
    TypeGuardCollection<ImageRaw>,
    TypeGuardCollection<TextureRaw>,
    TypeGuardCollection<GraphicsPipelineRaw>,
    TypeGuardCollection<DescriptorPoolDataRaw>,
    TypeGuardCollection<PersistentCommandPoolRaw>,
    TypeGuardCollection<FramebufferRaw>,
    Nil
];

#[derive(Debug)]
pub struct ResourceStorage {
    storage: ResourceStorageList,
}

impl Default for ResourceStorage {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
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
    pub unsafe fn destroy_raw_resource<R: 'static, M: Marker>(
        &mut self,
        context: &Context,
        index: RawIndex,
    ) -> ResourceResult<()>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context>,
        ResourceStorageList: Contains<TypeGuardCollection<R>, M>,
    {
        let index = unsafe { GenIndex::<TypeGuard<R>>::from_inner(index) };
        let _ = self
            .storage
            .get_mut()
            .pop(index)?
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
        self.destroy_resource_storage::<TextureRaw, _>(context)?;
        self.destroy_resource_storage::<ImageRaw, _>(context)?;
        self.destroy_resource_storage::<BufferRaw, _>(context)?;
        self.destroy_resource_storage::<MemoryRaw, _>(context)?;
        self.destroy_resource_storage::<GraphicsPipelineRaw, _>(context)?;
        self.destroy_resource_storage::<DescriptorPoolDataRaw, _>(context)?;
        self.destroy_resource_storage::<PersistentCommandPoolRaw, _>(context)?;
        self.destroy_resource_storage::<FramebufferRaw, _>(context)?;
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

pub type TypeUniqueRawCollection<R> = RefCell<TypeMap<<R as TypeUniqueResource>::RawType>>;
pub type TypeUniqueResourceStorageList = list_type![
    RefCell<TypeMap<RenderPassRaw>>,
    RefCell<TypeMap<DescriptorSetLayoutRaw>>,
    RefCell<TypeMap<PipelineLayoutRaw>>,
    RefCell<TypeMap<TransientCommandPoolRaw>>,
    Nil
];

pub trait TypeUniqueResource:
    FromGuard<Inner = Self::RawType>
    + for<'a, 'b> Create<Context<'a> = &'a Context, Config<'b> = (), CreateError = ResourceError>
{
    type RawType: Clone + Copy + for<'a> Destroy<Context<'a> = Self::Context<'a>>;
}

#[derive(Debug)]
pub struct TypeUniqueResourceStorage {
    storage: TypeUniqueResourceStorageList,
}

impl Default for TypeUniqueResourceStorage {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl TypeUniqueResourceStorage {
    #[inline]
    pub fn new() -> Self {
        Self {
            storage: TypeUniqueResourceStorageList::default(),
        }
    }

    #[inline]
    pub fn get_type_unique_resource<'a, R: TypeUniqueResource, M: Marker>(&self) -> Option<R>
    where
        TypeUniqueResourceStorageList: Contains<TypeUniqueRawCollection<R>, M>,
    {
        self.storage.get().borrow().get::<R>()
    }

    #[inline]
    pub fn create_type_unique_resource<'a, R: TypeUniqueResource, M: Marker>(
        &self,
        context: &Context,
    ) -> ResourceResult<R>
    where
        TypeUniqueResourceStorageList: Contains<TypeUniqueRawCollection<R>, M>,
    {
        let item = R::create((), context)?;
        self.storage.get().borrow_mut().insert(item);
        Ok(self.storage.get().borrow().get::<R>().unwrap())
    }

    #[inline]
    pub fn get_or_create_type_unique_resource<'a, R: TypeUniqueResource, M: Marker>(
        &self,
        context: &Context,
    ) -> ResourceResult<R>
    where
        TypeUniqueResourceStorageList: Contains<TypeUniqueRawCollection<R>, M>,
    {
        let item = self.storage.get().borrow().get::<R>();
        if let Some(value) = item {
            Ok(value)
        } else {
            self.create_type_unique_resource(context)
        }
    }

    #[inline]
    pub fn destroy_type_unique_resource<'a, R: TypeUniqueResource, M: Marker>(
        &self,
        context: &Context,
    ) -> ResourceResult<()>
    where
        TypeUniqueResourceStorageList: Contains<TypeUniqueRawCollection<R>, M>,
    {
        let mut item = self.storage.get().borrow_mut().remove::<R>();
        let _ = item.destroy(context);
        Ok(())
    }

    #[inline]
    fn destroy_type_unique_resource_storage<R: 'static, M: Marker>(
        &mut self,
        context: &Context,
    ) -> Result<(), CollectionDestroyError<R>>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context>,
        TypeUniqueResourceStorageList: Contains<RefCell<TypeMap<R>>, M>,
    {
        self.storage.get_mut().borrow_mut().destroy(context)
    }
}

impl Destroy for TypeUniqueResourceStorage {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.destroy_type_unique_resource_storage::<RenderPassRaw, _>(context);
        let _ = self.destroy_type_unique_resource_storage::<PipelineLayoutRaw, _>(context);
        let _ = self.destroy_type_unique_resource_storage::<DescriptorSetLayoutRaw, _>(context);
        let _ = self.destroy_type_unique_resource_storage::<TransientCommandPoolRaw, _>(context);
        Ok(())
    }
}
