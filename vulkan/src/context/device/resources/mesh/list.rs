use std::error::Error;

use crate::context::{
    device::{
        memory::AllocReq,
        raw::allocator::AllocatorIndex,
        resources::{DummyPack, PartialBuilder},
    },
    Context,
};
use graphics::model::{Mesh, MeshTypeList, Vertex};
use type_kit::{Cons, Create, Destroy, Nil, TypedNil};

use super::{MeshPack, MeshPackPartial, MeshPackRef};

pub trait MeshPackList: for<'a> Destroy<Context<'a> = &'a Context> {
    fn try_get<V: Vertex>(&self) -> Option<MeshPackRef<V>>;
}

impl MeshPackList for TypedNil<DummyPack> {
    fn try_get<V: Vertex>(&self) -> Option<MeshPackRef<V>> {
        None
    }
}

impl<V: Vertex, N: MeshPackList> MeshPackList for Cons<Option<MeshPack<V>>, N> {
    fn try_get<T: Vertex>(&self) -> Option<MeshPackRef<T>> {
        self.head
            .as_ref()
            .and_then(|pack| pack.try_into().ok())
            .or_else(|| self.tail.try_get::<T>())
    }
}

pub trait MeshPackListBuilder: MeshTypeList {
    type Pack: MeshPackList;

    fn prepare(
        &self,
        context: &Context,
    ) -> Result<impl MeshPackListPartial<Pack = Self::Pack>, Box<dyn Error>>;
}

impl MeshPackListBuilder for Nil {
    type Pack = TypedNil<DummyPack>;

    fn prepare(
        &self,
        _context: &Context,
    ) -> Result<impl MeshPackListPartial<Pack = Self::Pack>, Box<dyn Error>> {
        Ok(Nil::new())
    }
}

impl<V: Vertex, N: MeshPackListBuilder> MeshPackListBuilder for Cons<Vec<Mesh<V>>, N> {
    type Pack = Cons<Option<MeshPack<V>>, N::Pack>;

    fn prepare(
        &self,
        context: &Context,
    ) -> Result<impl MeshPackListPartial<Pack = Self::Pack>, Box<dyn Error>> {
        let meshes = self.get();
        let partial = if !meshes.is_empty() {
            Some(MeshPackPartial::prepare(self.get(), context)?)
        } else {
            None
        };
        Ok(Cons {
            head: partial,
            tail: self.tail.prepare(context)?,
        })
    }
}

pub trait MeshPackListPartial: Sized {
    type Pack: MeshPackList;

    fn get_memory_requirements(&self) -> Vec<AllocReq>;

    fn allocate(
        self,
        context: &Context,
        allocator: AllocatorIndex,
    ) -> Result<Self::Pack, Box<dyn Error>>;
}

impl MeshPackListPartial for Nil {
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

impl<'a, V: Vertex, N: MeshPackListPartial> MeshPackListPartial
    for Cons<Option<MeshPackPartial<'a, V>>, N>
{
    type Pack = Cons<Option<MeshPack<V>>, N::Pack>;

    fn get_memory_requirements(&self) -> Vec<AllocReq> {
        let mut alloc_reqs = self.tail.get_memory_requirements();
        if let Some(partial) = &self.head {
            alloc_reqs.extend(partial.requirements());
        }
        alloc_reqs
    }

    fn allocate(
        self,
        context: &Context,
        allocator: AllocatorIndex,
    ) -> Result<Self::Pack, Box<dyn Error>> {
        let Self { head, tail } = self;
        let pack = if let Some(partial) = head {
            Some(MeshPack::create((partial, allocator), context)?)
        } else {
            None
        };
        Ok(Cons {
            head: pack,
            tail: tail.allocate(context, allocator)?,
        })
    }
}
