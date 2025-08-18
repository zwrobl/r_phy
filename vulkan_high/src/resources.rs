mod common;
mod material;
mod mesh;
mod pipeline;
mod skybox;

pub use common::*;
pub use material::*;
pub use mesh::*;
pub use pipeline::*;
pub use skybox::*;

use std::{convert::Infallible, marker::PhantomData};

use type_kit::{Create, CreateResult, Destroy, DestroyResult};

use vulkan_low::{
    Context,
    error::VkResult,
    memory::allocator::{AllocatorBuilder, AllocatorIndex},
    resources::Partial,
};

use crate::renderer::Renderer;

pub struct DummyPack {}

impl Create for DummyPack {
    type Config<'a> = ();
    type CreateError = Infallible;

    fn create<'a, 'b>(_: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
        unreachable!()
    }
}

impl Destroy for DummyPack {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, _: Self::Context<'a>) -> DestroyResult<Self> {
        unreachable!()
    }
}

impl Partial for DummyPack {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, _builder: &mut B) {
        unreachable!()
    }
}

pub struct ResourcePack<
    R: Renderer,
    M: MaterialPackList,
    V: MeshPackList,
    P: GraphicsPipelinePackList,
> {
    pub materials: M,
    pub meshes: V,
    pub pipelines: P,
    _phantom: PhantomData<R>,
}

pub struct ResourcePackPartial<
    'a,
    R: Renderer,
    M: MaterialPackList,
    V: MeshPackList,
    P: GraphicsPipelineListBuilder,
    PM: MaterialPackListPartial<Pack = M>,
    PV: MeshPackListPartial<Pack = V>,
> where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    materials: PM,
    meshes: PV,
    pipelines: P,
    allocator: Option<AllocatorIndex>,
    _phantom: PhantomData<&'a R>,
}

impl<
    'a,
    R: Renderer,
    M: MaterialPackList,
    V: MeshPackList,
    P: GraphicsPipelineListBuilder,
    PM: MaterialPackListPartial<Pack = M>,
    PV: MeshPackListPartial<Pack = V>,
> ResourcePackPartial<'a, R, M, V, P, PM, PV>
where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    pub fn with_allocator<A: Into<AllocatorIndex>>(self, allocator: A) -> Self {
        Self {
            allocator: Some(allocator.into()),
            ..self
        }
    }

    pub fn build(self, context: &Context) -> VkResult<ResourcePack<R, M, V, P::Pack<R>>> {
        let Self {
            materials,
            meshes,
            pipelines,
            allocator,
            ..
        } = self;
        let meshes = meshes.allocate(context, allocator)?;
        let materials = materials.allocate(context, allocator)?;
        let pipelines = pipelines.build(context)?;
        Ok(ResourcePack {
            materials,
            meshes,
            pipelines,
            _phantom: PhantomData,
        })
    }
}

impl<
    'a,
    R: Renderer,
    M: MaterialPackList,
    V: MeshPackList,
    P: GraphicsPipelineListBuilder,
    PM: MaterialPackListPartial<Pack = M>,
    PV: MeshPackListPartial<Pack = V>,
> Destroy for ResourcePackPartial<'a, R, M, V, P, PM, PV>
where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    type Context<'b> = &'b Context;
    type DestroyError = Infallible;
    fn destroy<'b>(&mut self, context: Self::Context<'b>) -> DestroyResult<Self> {
        let _ = self.materials.destroy(context);
        let _ = self.meshes.destroy(context);
        Ok(())
    }
}
impl<
    'a,
    R: Renderer,
    M: MaterialPackList,
    V: MeshPackList,
    P: GraphicsPipelineListBuilder,
    PM: MaterialPackListPartial<Pack = M>,
    PV: MeshPackListPartial<Pack = V>,
> Partial for ResourcePackPartial<'a, R, M, V, P, PM, PV>
where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.materials.register_memory_requirements(builder);
        self.meshes.register_memory_requirements(builder);
    }
}

type PackPartial<'a, R, M, V, P, MB, MV> = ResourcePackPartial<
    'a,
    R,
    M,
    V,
    P,
    <MB as MaterialPackListBuilder>::Partial<'a>,
    <MV as MeshPackListBuilder>::Partial<'a>,
>;

impl<R: Renderer, M: MaterialPackList, V: MeshPackList, P: GraphicsPipelinePackList>
    ResourcePack<R, M, V, P>
{
    pub fn prepare<
        'a,
        MB: MaterialPackListBuilder<Pack = M>,
        MV: MeshPackListBuilder<Pack = V>,
        MP: GraphicsPipelineListBuilder<Pack<R> = P>,
    >(
        context: &Context,
        materials: &'a MB,
        meshes: &'a MV,
        pipelines: MP,
    ) -> VkResult<PackPartial<'a, R, M, V, MP, MB, MV>> {
        let materials = materials.prepare(context)?;
        let meshes = meshes.prepare(context)?;
        Ok(ResourcePackPartial {
            materials,
            meshes,
            pipelines,
            allocator: None,
            _phantom: PhantomData,
        })
    }
}

impl<R: Renderer, M: MaterialPackList, V: MeshPackList, P: GraphicsPipelinePackList> Destroy
    for ResourcePack<R, M, V, P>
{
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.materials.destroy(context);
        let _ = self.meshes.destroy(context);
        self.pipelines.destroy(context);
        Ok(())
    }
}
