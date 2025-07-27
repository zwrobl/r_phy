use std::error::Error;

use graphics::model::{MaterialCollection, MaterialTypeList};
use type_kit::{Cons, Destroy, Nil, TypedNil};
use vulkan_low::{
    device::raw::{
        allocator::{AllocatorBuilder, AllocatorIndex},
        resources::image::Image2DReader,
        Partial,
    },
    Context,
};

use crate::resources::{allocate_material_pack_memory, prepare_material_pack, DummyPack};

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
            Some(prepare_material_pack(context, materials)?)
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

    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B);

    fn allocate(
        self,
        context: &Context,
        allocator: AllocatorIndex,
    ) -> Result<Self::Pack, Box<dyn Error>>;
}

impl MaterialPackListPartial for Nil {
    type Pack = TypedNil<DummyPack>;

    fn register_memory_requirements<B: AllocatorBuilder>(&self, _builder: &mut B) {}

    fn allocate(
        self,
        _context: &Context,
        _allocator: AllocatorIndex,
    ) -> Result<Self::Pack, Box<dyn Error>> {
        Ok(TypedNil::new())
    }
}

impl<'a, M: Material, N: MaterialPackListPartial> MaterialPackListPartial
    for Cons<Option<MaterialPackPartial<'a, M, Image2DReader<'a>>>, N>
{
    type Pack = Cons<Option<MaterialPack<M>>, N::Pack>;

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
        let pack = if let Some(pack) = head {
            Some(allocate_material_pack_memory(context, pack, allocator)?)
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
