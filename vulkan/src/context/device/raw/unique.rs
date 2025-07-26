pub mod layout;
pub mod render_pass;

use std::{cell::RefCell, convert::Infallible};

use super::resources::command::TransientCommandPoolRaw;

use type_kit::{
    list_type, CollectionDestroyError, Cons, Contains, Create, Destroy, DestroyResult, FromGuard,
    Marker, Nil, TypeMap,
};

use crate::context::{
    device::raw::unique::{
        layout::{DescriptorSetLayoutRaw, PipelineLayoutRaw},
        render_pass::RenderPassRaw,
    },
    error::{ResourceError, ResourceResult},
    Context,
};

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

pub type Raw<R> = <R as TypeUniqueResource>::RawType;

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
