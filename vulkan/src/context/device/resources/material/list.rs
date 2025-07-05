use std::error::Error;

use crate::context::{
    device::{memory::AllocReq, raw::allocator::AllocatorIndex, resources::DummyPack},
    Context,
};
use graphics::model::{MaterialCollection, MaterialTypeList};
use type_kit::{Cons, Destroy, Nil, TypedNil};

use super::{Material, MaterialPack, MaterialPackPartial, MaterialPackRef};

pub trait MaterialPackListBuilder: MaterialTypeList {
    type Pack: MaterialPackList;

    fn prepare(
        &self,
        device: &Context,
    ) -> Result<impl MaterialPackListPartial<Pack = Self::Pack>, Box<dyn Error>>;
}

impl MaterialPackListBuilder for Nil {
    type Pack = TypedNil<DummyPack>;

    fn prepare(
        &self,
        _device: &Context,
    ) -> Result<impl MaterialPackListPartial<Pack = Self::Pack>, Box<dyn Error>> {
        Ok(Nil::new())
    }
}

impl<M: Material, N: MaterialPackListBuilder> MaterialPackListBuilder for Cons<Vec<M>, N> {
    type Pack = Cons<Option<MaterialPack<M>>, N::Pack>;

    fn prepare(
        &self,
        context: &Context,
    ) -> Result<impl MaterialPackListPartial<Pack = Self::Pack>, Box<dyn Error>> {
        let materials = self.get();
        let partial = if !materials.is_empty() {
            Some(context.prepare_material_pack(materials)?)
        } else {
            None
        };
        Ok(Cons {
            head: partial,
            tail: self.next().prepare(context)?,
        })
    }
}

pub trait MaterialPackListPartial: Sized {
    type Pack: MaterialPackList;

    fn get_memory_requirements(&self) -> Vec<AllocReq>;

    fn allocate(
        self,
        context: &Context,
        allocator: AllocatorIndex,
    ) -> Result<Self::Pack, Box<dyn Error>>;
}

impl MaterialPackListPartial for Nil {
    type Pack = TypedNil<DummyPack>;

    fn get_memory_requirements(&self) -> Vec<AllocReq> {
        vec![]
    }

    fn allocate(
        self,
        _context: &Context,
        _allocator: AllocatorIndex,
    ) -> Result<Self::Pack, Box<dyn Error>> {
        Ok(TypedNil::new())
    }
}

impl<'a, M: Material, N: MaterialPackListPartial> MaterialPackListPartial
    for Cons<Option<MaterialPackPartial<'a, M>>, N>
{
    type Pack = Cons<Option<MaterialPack<M>>, N::Pack>;

    fn get_memory_requirements(&self) -> Vec<AllocReq> {
        let mut alloc_reqs = self.tail.get_memory_requirements();
        if let Some(partial) = &self.head {
            alloc_reqs.extend(partial.get_alloc_req());
        }
        alloc_reqs
    }

    fn allocate(
        self,
        context: &Context,
        allocator: AllocatorIndex,
    ) -> Result<Self::Pack, Box<dyn Error>> {
        let Self { head, tail } = self;
        let pack = if let Some(pack) = head {
            Some(context.allocate_material_pack_memory(pack, allocator)?)
        } else {
            None
        };
        Ok(Cons {
            head: pack,
            tail: tail.allocate(context, allocator)?,
        })
    }
}

pub trait MaterialPackList: for<'a> Destroy<Context<'a> = &'a Context> {
    fn try_get<M: Material>(&self) -> Option<MaterialPackRef<M>>;
}

impl MaterialPackList for TypedNil<DummyPack> {
    fn try_get<T: Material>(&self) -> Option<MaterialPackRef<T>> {
        None
    }
}

impl<M: Material, N: MaterialPackList> MaterialPackList for Cons<Option<MaterialPack<M>>, N> {
    fn try_get<T: Material>(&self) -> Option<MaterialPackRef<T>> {
        self.head
            .as_ref()
            .and_then(|pack| pack.try_into().ok())
            .or_else(|| self.tail.try_get::<T>())
    }
}
