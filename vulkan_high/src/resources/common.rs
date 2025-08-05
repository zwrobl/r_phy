use std::{
    convert::Infallible,
    sync::{Once, RwLock},
};

use graphics::model::{CommonVertex, Mesh, MeshBuilder};

use math::types::Vector3;
use physics::shape;
use strum::EnumCount;
use type_kit::{Create, CreateResult, Destroy};
use vulkan_low::{
    memory::allocator::{AllocatorBuilder, AllocatorIndex},
    resources::{
        command::{Level, Lifetime, Operation, RecordingCommand},
        error::ResourceError,
        Partial,
    },
    Context,
};

use crate::resources::{bind_mesh_pack, MeshPack, MeshPackPartial};
use strum::IntoEnumIterator;

#[derive(Debug, Clone, Copy, strum::EnumCount, strum::EnumIter)]
pub enum CommonMesh {
    Plane,
    Cube,
}

impl CommonMesh {
    fn get_data(self) -> Mesh<CommonVertex> {
        match self {
            CommonMesh::Plane => MeshBuilder::plane_subdivided(
                0,
                2.0 * Vector3::y(),
                2.0 * Vector3::x(),
                Vector3::zero(),
                false,
            )
            .offset(Vector3::new(-1.0, -1.0, 0.0))
            .build(),
            CommonMesh::Cube => shape::Cube::new(1.0).into(),
        }
    }

    fn get_meshes() -> &'static [Mesh<CommonVertex>] {
        static MESHES: RwLock<Option<Vec<Mesh<CommonVertex>>>> = RwLock::new(None);
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            MESHES
                .write()
                .unwrap()
                .replace(CommonMesh::iter().map(CommonMesh::get_data).collect());
        });
        MESHES
            .read()
            .map(|m| {
                let data = m.as_ref().unwrap().as_ptr();
                unsafe { std::slice::from_raw_parts(data, CommonMesh::COUNT) }
            })
            .unwrap()
    }
}

pub struct CommonResourcesPartial {
    partial: MeshPackPartial<'static, CommonVertex>,
}

impl Partial for CommonResourcesPartial {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.partial.register_memory_requirements(builder);
    }
}

impl Create for CommonResourcesPartial {
    type Config<'a> = ();
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(_: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let partial = MeshPackPartial::create(CommonMesh::get_meshes(), context)?;
        Ok(Self { partial })
    }
}

impl Destroy for CommonResourcesPartial {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        let _ = self.partial.destroy(context);
        Ok(())
    }
}

pub struct CommonResources {
    meshes: MeshPack<CommonVertex>,
}

impl Create for CommonResources {
    type Config<'a> = (CommonResourcesPartial, Option<AllocatorIndex>);
    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (CommonResourcesPartial { partial }, allocator) = config;
        let pack = MeshPack::create((partial, allocator), context)?;
        Ok(Self { meshes: pack })
    }
}

impl CommonResources {
    #[inline]
    pub fn draw<'a, T: Lifetime, L: Level, O: Operation>(
        &self,
        context: &Context,
        command: RecordingCommand<'a, T, L, O>,
        mesh: CommonMesh,
    ) -> RecordingCommand<'a, T, L, O> {
        bind_mesh_pack(context, command, &self.meshes).draw_indexed(self.meshes.get(mesh as usize))
    }
}

impl Destroy for CommonResources {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        let _ = self.meshes.destroy(context);
        Ok(())
    }
}
