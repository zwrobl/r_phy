use std::{cell::RefCell, convert::Infallible, fmt::Debug};

use super::{
    buffer::BufferRaw,
    command::{PersistentCommandPoolRaw, TransientCommandPoolRaw},
    descriptor::DescriptorPoolDataRaw,
    framebuffer::FramebufferRaw,
    image::{ImageRaw, TextureRaw},
    layout::{DescriptorSetLayoutRaw, PipelineLayoutRaw},
    pipeline::GraphicsPipelineRaw,
    render_pass::RenderPassRaw,
    swapchain::SwapchainRaw,
};
use type_kit::{
    list_type, BorrowList, BorrowedGuard, CollectionDestroyError, Cons, Contains, Create, Destroy,
    DestroyResult, FromGuard, GenCollection, GenCollectionResult, GenIndex, GuardCell, GuardVec,
    IndexList, Marked, Marker, Nil, TypeGuard, TypeMap, TypedIndex,
};

use crate::{
    resources::{
        error::{ResourceError, ResourceResult},
        RawIndex, Resource, ResourceIndex,
    },
    Context,
};

pub type RawCollection<R> = <R as Resource>::RawCollection;
pub type ResourceStorageList = list_type![
    GuardVec<BufferRaw>,
    GuardVec<ImageRaw>,
    GuardVec<TextureRaw>,
    GuardVec<GraphicsPipelineRaw>,
    GuardVec<DescriptorPoolDataRaw>,
    GuardVec<PersistentCommandPoolRaw>,
    GuardVec<FramebufferRaw>,
    GuardCell<SwapchainRaw>,
    Nil
];

pub type BorrowRef<'a, T> =
    <<<T as ResourceIndexList>::List as IndexList<ResourceStorageList>>::Borrowed as BorrowList<
        ResourceStorageList,
    >>::InnerRef<'a>;

pub type BorrowMut<'a, T> =
    <<<T as ResourceIndexList>::List as IndexList<ResourceStorageList>>::Borrowed as BorrowList<
        ResourceStorageList,
    >>::InnerMut<'a>;

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
    pub fn push_resource<R: Resource, M: Marker>(
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
        let resource = R::try_from_guard(self.storage.borrow_mut().get_mut().pop(index.index)?)
            .map_err(R::wrap_guard_error)?;
        Ok(resource)
    }

    /// # Safety
    /// This method allows user to remove resource of type R using raw index.
    /// The caller must ensure that the index corresponds to a valid resource of type R.
    #[inline]
    pub unsafe fn pop_raw_resource<R, C: GenCollection<TypeGuard<R>>, M: Marker>(
        &self,
        index: RawIndex,
    ) -> ResourceResult<TypeGuard<R>>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context> + 'static,
        ResourceStorageList: Contains<C, M>,
    {
        let index = unsafe { GenIndex::<TypeGuard<R>, _>::from_inner(index) };
        let resource = self.storage.borrow_mut().get_mut().pop(index)?;
        Ok(resource)
    }

    #[inline]
    pub fn operate_ref<I: ResourceIndexList, R, E, F: FnOnce(BorrowRef<'_, I>) -> Result<R, E>>(
        &self,
        index: I,
        f: F,
    ) -> GenCollectionResult<Result<R, E>> {
        let index_list = index.into_index_list();
        let borrowed = index_list.get_borrowed(&mut self.storage.borrow_mut())?;
        let result = f(borrowed.inner_ref());
        borrowed.put_back(&mut self.storage.borrow_mut())?;
        Ok(result)
    }

    #[inline]
    pub fn operate_mut<I: ResourceIndexList, R, E, F: FnOnce(BorrowMut<'_, I>) -> Result<R, E>>(
        &self,
        index: I,
        f: F,
    ) -> GenCollectionResult<Result<R, E>> {
        let index_list = index.into_index_list();
        let mut borrowed = index_list.get_borrowed(&mut self.storage.borrow_mut())?;
        let result = f(borrowed.inner_mut());
        borrowed.put_back(&mut self.storage.borrow_mut())?;
        Ok(result)
    }

    #[inline]
    fn destroy_vec_resource_storage<R, M: Marker>(&self, context: &Context) -> DestroyResult<R>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context> + 'static,
        ResourceStorageList: Contains<GuardVec<R>, M>,
    {
        let items = self.storage.borrow_mut().get_mut().drain();
        items
            .into_iter()
            .try_for_each(|mut item| item.destroy(context))
    }

    #[inline]
    fn destroy_cell_resource_storage<R, M: Marker>(&self, context: &Context) -> DestroyResult<R>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context> + 'static,
        ResourceStorageList: Contains<GuardCell<R>, M>,
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

impl Default for ResourceIndexListBuilder<Nil> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
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

#[macro_export]
macro_rules! index_list {
    [$($indices:expr),*] => {
        ResourceIndexListBuilder::new()
        $(.push($indices))*
        .build()
    };
}

pub trait ResourceIndexList {
    type List: IndexList<ResourceStorageList>;
    type BorrowList: BorrowList<ResourceStorageList>;

    fn into_index_list(self) -> Self::List;
}

impl ResourceIndexList for Nil {
    type List = Nil;
    type BorrowList = Nil;

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
    type BorrowList = Cons<Marked<BorrowedGuard<R, R::RawCollection>, M>, N::BorrowList>;

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
    pub fn get_type_unique_resource<R: TypeUniqueResource, M: Marker>(&self) -> Option<R>
    where
        TypeUniqueResourceStorageList: Contains<TypeUniqueRawCollection<R>, M>,
    {
        self.storage.borrow().get().get::<R>()
    }

    #[inline]
    pub fn create_type_unique_resource<R: TypeUniqueResource, M: Marker>(
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
    pub fn get_or_create_type_unique_resource<R: TypeUniqueResource, M: Marker>(
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
    pub fn destroy_type_unique_resource<R: TypeUniqueResource, M: Marker>(
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
    fn destroy_type_unique_resource_storage<R, M: Marker>(
        &self,
        context: &Context,
    ) -> Result<(), CollectionDestroyError<R>>
    where
        for<'a> R: Destroy<Context<'a> = &'a Context> + 'static,
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
