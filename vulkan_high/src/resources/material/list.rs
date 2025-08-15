use std::any::type_name;

use graphics::model::{MaterialCollection, MaterialTypeList};
use type_kit::{Cons, Destroy, Nil, TypedNil};
use vulkan_low::{
    error::VkResult,
    memory::allocator::{AllocatorBuilder, AllocatorIndex},
    resources::Partial,
    Context,
};

use crate::resources::{allocate_material_pack_memory, prepare_material_pack, DummyPack};

use super::{Material, MaterialPack, MaterialPackPartial, MaterialPackRef};

pub trait MaterialPackListBuilder: MaterialTypeList {
    type Pack: MaterialPackList;
    type Partial<'a>: MaterialPackListPartial<Pack = Self::Pack>
        + for<'b> Destroy<Context<'b> = &'b Context>;

    fn prepare<'a>(&'a self, device: &Context) -> VkResult<Self::Partial<'a>>;
}

impl MaterialPackListBuilder for Nil {
    type Pack = TypedNil<DummyPack>;
    type Partial<'a> = TypedNil<DummyPack>;

    fn prepare<'a>(&'a self, _device: &Context) -> VkResult<Self::Partial<'a>> {
        Ok(TypedNil::new())
    }
}

impl<M: Material, N: MaterialPackListBuilder> MaterialPackListBuilder for Cons<Vec<M>, N> {
    type Pack = Cons<Option<MaterialPack<M>>, N::Pack>;
    type Partial<'a> = Cons<Option<MaterialPackPartial<'a, M>>, N::Partial<'a>>;

    fn prepare<'a>(&'a self, context: &Context) -> VkResult<Self::Partial<'a>> {
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

    fn allocate(self, context: &Context, allocator: Option<AllocatorIndex>)
        -> VkResult<Self::Pack>;
}

impl MaterialPackListPartial for TypedNil<DummyPack> {
    type Pack = TypedNil<DummyPack>;

    fn register_memory_requirements<B: AllocatorBuilder>(&self, _builder: &mut B) {}

    fn allocate(
        self,
        _context: &Context,
        _allocator: Option<AllocatorIndex>,
    ) -> VkResult<Self::Pack> {
        Ok(TypedNil::new())
    }
}

impl<'a, M: Material, N: MaterialPackListPartial> MaterialPackListPartial
    for Cons<Option<MaterialPackPartial<'a, M>>, N>
{
    type Pack = Cons<Option<MaterialPack<M>>, N::Pack>;

    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.head.register_memory_requirements(builder);
        self.tail.register_memory_requirements(builder);
    }

    fn allocate(
        self,
        context: &Context,
        allocator: Option<AllocatorIndex>,
    ) -> VkResult<Self::Pack> {
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

impl<M: Material, T: Material> TryFrom<&Option<MaterialPack<M>>> for MaterialPackRef<T> {
    type Error = &'static str;

    fn try_from(value: &Option<MaterialPack<M>>) -> Result<Self, Self::Error> {
        if let Some(pack) = value {
            pack.try_into()
        } else {
            Err("Option<MaterialPack> is None")
        }
    }
}
pub trait MaterialPackList: for<'a> Destroy<Context<'a> = &'a Context> {
    fn try_get<M: Material>(&self) -> Option<MaterialPackRef<M>>;
    fn get<M: Material>(&self) -> MaterialPackRef<M>;
}

impl MaterialPackList for TypedNil<DummyPack> {
    fn try_get<T: Material>(&self) -> Option<MaterialPackRef<T>> {
        None
    }

    fn get<T: Material>(&self) -> MaterialPackRef<T> {
        panic!(
            "No material pack found for the requested type: {}",
            type_name::<T>()
        );
    }
}

impl<M: Material, N: MaterialPackList> MaterialPackList for Cons<Option<MaterialPack<M>>, N> {
    fn try_get<T: Material>(&self) -> Option<MaterialPackRef<T>> {
        if let Ok(pack) = (&self.head).try_into() {
            Some(pack)
        } else {
            self.tail.try_get::<T>()
        }
    }

    fn get<T: Material>(&self) -> MaterialPackRef<T> {
        if let Ok(pack) = (&self.head).try_into() {
            pack
        } else {
            self.tail.get::<T>()
        }
    }
}
