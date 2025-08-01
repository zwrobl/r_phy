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
use swapchain::SwapchainRaw;
use type_kit::{
    list_type, BorrowCollection, BorrowList, CollectionDestroyError, Cons, Contains, Create,
    Destroy, DestroyResult, FromGuard, GenCell, GenCollectionResult, GenIndex, GenIndexRaw,
    GuardCollectionT, GuardIndex, IndexList, Marked, Marker, Nil, TypeGuard, TypeGuardCollection,
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
    type RawCollection: GuardCollectionT<Self::RawType>;
}

pub type Raw<R> = <R as Resource>::RawType;

pub struct ResourceIndex<R: Resource> {
    index: GuardIndex<R, R::RawCollection>,
}

impl<R: Resource> ResourceIndex<R> {
    #[inline]
    pub fn unwrap(self) -> GuardIndex<R, R::RawCollection> {
        self.index
    }

    #[inline]
    pub fn mark<L, M: Marker>(self) -> Marked<Self, M>
    where
        L: Contains<R::RawCollection, M>,
    {
        Marked::new(self)
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
            index: GuardIndex::<R, R::RawCollection>::from_inner(inner),
        }
    }
}

pub type RawCollection<R> = <R as Resource>::RawCollection;
pub type ResourceStorageList = list_type![
    TypeGuardCollection<MemoryRaw>,
    TypeGuardCollection<BufferRaw>,
    TypeGuardCollection<ImageRaw>,
    TypeGuardCollection<TextureRaw>,
    TypeGuardCollection<GraphicsPipelineRaw>,
    TypeGuardCollection<DescriptorPoolDataRaw>,
    TypeGuardCollection<PersistentCommandPoolRaw>,
    TypeGuardCollection<FramebufferRaw>,
    GenCell<TypeGuard<SwapchainRaw>>,
    Nil
];

#[derive(Debug)]
pub struct ResourceStorage {
    storage: RefCell<ResourceStorageList>,
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
            storage: RefCell::new(ResourceStorageList::default()),
        }
    }

    #[inline]
    pub fn push_resource<'a, R: Resource, M: Marker>(
        &self,
        resource: R,
    ) -> ResourceResult<ResourceIndex<R>>
    where
        ResourceStorageList: Contains<RawCollection<R>, M>,
    {
        let index = self
            .storage
            .borrow_mut()
            .get_mut()
            .push(resource.into_guard())?;
        Ok(ResourceIndex { index })
    }

    #[inline]
    pub fn pop_resource<R: Resource, M: Marker>(&self, index: ResourceIndex<R>) -> ResourceResult<R>
    where
        ResourceStorageList: Contains<RawCollection<R>, M>,
    {
        // TODO: Proper type guard type check should be performed here
        let resource = unsafe {
            R::from_inner(
                self.storage
                    .borrow_mut()
                    .get_mut()
                    .pop(index.index)?
                    .into_inner(),
            )
        };
        Ok(resource)
    }

    #[inline]
    pub unsafe fn pop_raw_resource<R: 'static, M: Marker>(
        &self,
        index: RawIndex,
    ) -> ResourceResult<R>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context>,
        ResourceStorageList: Contains<TypeGuardCollection<R>, M>,
    {
        let index = unsafe { GenIndex::<TypeGuard<R>, _>::from_inner(index) };
        let resource = self.storage.borrow_mut().get_mut().pop(index)?.into_inner();
        Ok(resource)
    }

    #[inline]
    pub fn opperate_ref<
        I: ResourceIndexList,
        R,
        E,
        F: FnOnce(&<I::List as IndexList<ResourceStorageList>>::Borrowed) -> Result<R, E>,
    >(
        &self,
        index: I,
        f: F,
    ) -> GenCollectionResult<Result<R, E>> {
        let index_list = index.into_index_list();
        let borrowed = index_list.get_borrowed(&mut self.storage.borrow_mut())?;
        let result = f(&borrowed);
        borrowed.put_back(&mut self.storage.borrow_mut())?;
        Ok(result)
    }

    #[inline]
    pub fn opperate_mut<
        I: ResourceIndexList,
        R,
        E,
        F: FnOnce(&mut <I::List as IndexList<ResourceStorageList>>::Borrowed) -> Result<R, E>,
    >(
        &self,
        index: I,
        f: F,
    ) -> GenCollectionResult<Result<R, E>> {
        let index_list = index.into_index_list();
        let mut borrowed = index_list.get_borrowed(&mut self.storage.borrow_mut())?;
        let result = f(&mut borrowed);
        borrowed.put_back(&mut self.storage.borrow_mut())?;
        Ok(result)
    }

    #[inline]
    fn destroy_vec_resource_storage<R: 'static, M: Marker>(
        &self,
        context: &Context,
    ) -> DestroyResult<R>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context>,
        ResourceStorageList: Contains<TypeGuardCollection<R>, M>,
    {
        let items = self.storage.borrow_mut().get_mut().drain();
        items
            .into_iter()
            .try_for_each(|mut item| item.destroy(context))
    }

    #[inline]
    fn destroy_cell_resource_storage<R: 'static, M: Marker>(
        &self,
        context: &Context,
    ) -> DestroyResult<R>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context>,
        ResourceStorageList: Contains<GenCell<TypeGuard<R>>, M>,
    {
        let resource = self.storage.borrow_mut().get_mut().drain();
        if let Some(mut item) = resource {
            item.destroy(context)?;
        };
        Ok(())
    }

    #[inline]
    pub fn destroy_storage(&self, context: &Context) -> Result<(), Infallible> {
        self.destroy_vec_resource_storage::<TextureRaw, _>(context)?;
        self.destroy_vec_resource_storage::<ImageRaw, _>(context)?;
        self.destroy_vec_resource_storage::<BufferRaw, _>(context)?;
        self.destroy_vec_resource_storage::<MemoryRaw, _>(context)?;
        self.destroy_vec_resource_storage::<GraphicsPipelineRaw, _>(context)?;
        self.destroy_vec_resource_storage::<DescriptorPoolDataRaw, _>(context)?;
        self.destroy_vec_resource_storage::<PersistentCommandPoolRaw, _>(context)?;
        self.destroy_vec_resource_storage::<FramebufferRaw, _>(context)?;
        self.destroy_cell_resource_storage::<SwapchainRaw, _>(context)?;
        Ok(())
    }
}

pub struct ResourceIndexListBuilder<I: ResourceIndexList> {
    list: I,
}

impl ResourceIndexListBuilder<Nil> {
    #[inline]
    pub fn new() -> Self {
        Self {
            list: Nil::default(),
        }
    }
}

impl<I: ResourceIndexList> ResourceIndexListBuilder<I> {
    #[inline]
    pub fn push<R: Resource, M: Marker>(
        self,
        index: ResourceIndex<R>,
    ) -> ResourceIndexListBuilder<Cons<Marked<ResourceIndex<R>, M>, I>>
    where
        ResourceStorageList: Contains<R::RawCollection, M>,
    {
        ResourceIndexListBuilder {
            list: Cons {
                head: Marked::new(index),
                tail: self.list,
            },
        }
    }

    #[inline]
    pub fn build(self) -> I {
        self.list
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
    ResourceStorageList: Contains<R::RawCollection, M>,
{
    type List = Cons<Marked<TypedIndex<R, R::RawCollection>, M>, N::List>;

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

pub type TypeUniqueRawCollection<R> = TypeMap<<R as TypeUniqueResource>::RawType>;
pub type TypeUniqueResourceStorageList = list_type![
    TypeMap<RenderPassRaw>,
    TypeMap<DescriptorSetLayoutRaw>,
    TypeMap<PipelineLayoutRaw>,
    TypeMap<TransientCommandPoolRaw>,
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
    storage: RefCell<TypeUniqueResourceStorageList>,
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
            storage: RefCell::new(TypeUniqueResourceStorageList::default()),
        }
    }

    #[inline]
    pub fn get_type_unique_resource<'a, R: TypeUniqueResource, M: Marker>(&self) -> Option<R>
    where
        TypeUniqueResourceStorageList: Contains<TypeUniqueRawCollection<R>, M>,
    {
        self.storage.borrow().get().get::<R>()
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
        self.storage.borrow_mut().get_mut().insert(item);
        Ok(self.storage.borrow().get().get::<R>().unwrap())
    }

    #[inline]
    pub fn get_or_create_type_unique_resource<'a, R: TypeUniqueResource, M: Marker>(
        &self,
        context: &Context,
    ) -> ResourceResult<R>
    where
        TypeUniqueResourceStorageList: Contains<TypeUniqueRawCollection<R>, M>,
    {
        let item = self.storage.borrow().get().get::<R>();
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
        let mut item = self.storage.borrow_mut().get_mut().remove::<R>();
        let _ = item.destroy(context);
        Ok(())
    }

    #[inline]
    fn destroy_type_unique_resource_storage<R: 'static, M: Marker>(
        &self,
        context: &Context,
    ) -> Result<(), CollectionDestroyError<R>>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context>,
        TypeUniqueResourceStorageList: Contains<TypeMap<R>, M>,
    {
        self.storage.borrow_mut().get_mut().destroy(context)
    }

    #[inline]
    pub fn destroy_storage(&self, context: &Context) -> Result<(), Infallible> {
        let _ = self.destroy_type_unique_resource_storage::<RenderPassRaw, _>(context);
        let _ = self.destroy_type_unique_resource_storage::<PipelineLayoutRaw, _>(context);
        let _ = self.destroy_type_unique_resource_storage::<DescriptorSetLayoutRaw, _>(context);
        let _ = self.destroy_type_unique_resource_storage::<TransientCommandPoolRaw, _>(context);
        Ok(())
    }
}
