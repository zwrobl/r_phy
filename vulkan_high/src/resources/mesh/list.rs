use std::{any::type_name, error::Error};

use graphics::model::{Mesh, MeshTypeList, Vertex};
use type_kit::{Cons, Create, Destroy, Nil, TypedNil};
use vulkan_low::{
    device::raw::{
        allocator::{AllocatorBuilder, AllocatorIndex},
        Partial,
    },
    Context,
};

use crate::resources::DummyPack;

use super::{MeshPack, MeshPackPartial, MeshPackRef};

pub trait MeshPackList: for<'a> Destroy<Context<'a> = &'a Context> {
    fn try_get<V: Vertex>(&self) -> Option<MeshPackRef<V>>;
    fn get<V: Vertex>(&self) -> MeshPackRef<V>;
}

impl MeshPackList for TypedNil<DummyPack> {
    fn try_get<V: Vertex>(&self) -> Option<MeshPackRef<V>> {
        None
    }
    fn get<V: Vertex>(&self) -> MeshPackRef<V> {
        panic!(
            "No mesh pack found for the requested type: {}",
            type_name::<V>()
        );
    }
}

impl<'a, V: Vertex, T: Vertex> TryFrom<&'a Option<MeshPack<V>>> for MeshPackRef<'a, T> {
    type Error = &'static str;

    fn try_from(value: &'a Option<MeshPack<V>>) -> Result<Self, Self::Error> {
        if let Some(pack) = value {
            pack.try_into()
        } else {
            Err("Option<MeshPack> is None")
        }
    }
}

impl<V: Vertex, N: MeshPackList> MeshPackList for Cons<Option<MeshPack<V>>, N> {
    fn try_get<T: Vertex>(&self) -> Option<MeshPackRef<T>> {
        (&self.head)
            .try_into()
            .ok()
            .or_else(|| self.tail.try_get::<T>())
    }

    fn get<T: Vertex>(&self) -> MeshPackRef<T> {
        if let Some(pack) = (&self.head).try_into().ok() {
            pack
        } else {
            self.tail.get::<T>()
        }
    }
}

pub trait MeshPackListBuilder: MeshTypeList {
    type Pack: MeshPackList;
    type Partial<'a>: MeshPackListPartial<Pack = Self::Pack>
        + for<'b> Destroy<Context<'b> = &'b Context>;

    fn prepare<'a>(&'a self, context: &Context) -> Result<Self::Partial<'a>, Box<dyn Error>>;
}

impl MeshPackListBuilder for Nil {
    type Pack = TypedNil<DummyPack>;
    type Partial<'a> = TypedNil<DummyPack>;

    fn prepare<'a>(&'a self, _context: &Context) -> Result<Self::Partial<'a>, Box<dyn Error>> {
        Ok(TypedNil::new())
    }
}

impl<V: Vertex, N: MeshPackListBuilder> MeshPackListBuilder for Cons<Vec<Mesh<V>>, N> {
    type Pack = Cons<Option<MeshPack<V>>, N::Pack>;
    type Partial<'a> = Cons<Option<MeshPackPartial<'a, V>>, N::Partial<'a>>;

    fn prepare<'a>(&'a self, context: &Context) -> Result<Self::Partial<'a>, Box<dyn Error>> {
        let meshes = self.get();
        let partial = if !meshes.is_empty() {
            Some(MeshPackPartial::create(self.get(), context)?)
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

    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B);

    fn allocate(
        self,
        context: &Context,
        allocator: AllocatorIndex,
    ) -> Result<Self::Pack, Box<dyn Error>>;
}

impl MeshPackListPartial for TypedNil<DummyPack> {
    type Pack = TypedNil<DummyPack>;

    fn allocate(
        self,
        _context: &Context,
        _allocator: AllocatorIndex,
    ) -> Result<Self::Pack, Box<dyn Error>> {
        Ok(TypedNil::new())
    }

    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, _builder: &mut B) {}
}

impl<'a, V: Vertex, N: MeshPackListPartial> MeshPackListPartial
    for Cons<Option<MeshPackPartial<'a, V>>, N>
{
    type Pack = Cons<Option<MeshPack<V>>, N::Pack>;

    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.head.register_memory_requirements(builder);
        self.tail.register_memory_requirements(builder);
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
