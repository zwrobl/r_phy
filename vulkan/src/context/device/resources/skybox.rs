use std::{convert::Infallible, ops::Deref, path::Path, sync::Once};

use graphics::{
    model::{CommonVertex, Mesh},
    renderer::camera::CameraMatrices,
};
use physics::shape;

use crate::context::{
    device::{
        descriptor::{DescriptorPool, DescriptorSetWriter, TextureDescriptorSet},
        pipeline::{
            GraphicsPipeline, GraphicsPipelineConfig, PipelineLayoutBuilder, ShaderDirectory,
        },
        raw::{allocator::AllocatorIndex, Partial},
        resources::{image::Texture2DPartial, MeshPackPartial},
    },
    error::{ResourceError, VkError},
    Context,
};
use type_kit::{Cons, Create, Destroy, DestroyResult, DropGuard, DropGuardError, Nil};

use super::{
    image::{ImageReader, Texture2D},
    MeshPack,
};

pub type LayoutSkybox =
    PipelineLayoutBuilder<Cons<TextureDescriptorSet, Nil>, Cons<CameraMatrices, Nil>>;

pub struct SkyboxPartial<'a> {
    cubemap: Texture2DPartial<'a>,
    cube: MeshPackPartial<'static, CommonVertex>,
}

pub struct Skybox<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> {
    cubemap: DropGuard<Texture2D>,
    pub mesh_pack: DropGuard<MeshPack<CommonVertex>>,
    pub descriptor: DropGuard<DescriptorPool<TextureDescriptorSet>>,
    pub pipeline: DropGuard<GraphicsPipeline<L>>,
}

const SKYBOX_SHADER: &'static str = "_resources/shaders/spv/skybox";

fn get_skybox_meshes() -> &'static [Mesh<CommonVertex>] {
    static mut CUBE: Option<[Mesh<CommonVertex>; 1]> = None;
    static INIT: Once = Once::new();
    unsafe {
        INIT.call_once(|| {
            if CUBE.is_none() {
                CUBE.replace([shape::Cube::new(1.0).into()]);
            }
        });
        CUBE.as_ref().unwrap()
    }
}

impl<'c> Create for SkyboxPartial<'c> {
    type Config<'a> = &'a Path;

    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        Ok(SkyboxPartial {
            cubemap: Texture2DPartial::create(ImageReader::cube(config)?, context)?,
            cube: MeshPackPartial::create(get_skybox_meshes(), context)?,
        })
    }
}

impl<'a> Partial for SkyboxPartial<'a> {
    #[inline]
    fn register_memory_requirements<B: crate::context::device::raw::allocator::AllocatorBuilder>(
        &self,
        builder: &mut B,
    ) {
        self.cube.register_memory_requirements(builder);
        self.cubemap.register_memory_requirements(builder);
    }
}

impl<'b> Destroy for SkyboxPartial<'b> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.cube.destroy(context);
        self.cubemap.destroy(context);
        Ok(())
    }
}

impl<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> Create for Skybox<L> {
    type Config<'a> = (SkyboxPartial<'a>, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (SkyboxPartial { cubemap, cube }, allocator) = config;
        let cubemap = Texture2D::create((cubemap, allocator), context)?;
        let descriptor = DescriptorPool::create(
            DescriptorSetWriter::<TextureDescriptorSet>::new(1)
                .write_images::<Texture2D, _>(std::slice::from_ref(&cubemap)),
            context,
        )?;
        let layout = context.get_pipeline_layout::<L::Layout>()?;
        let modules = ShaderDirectory::new(Path::new(SKYBOX_SHADER));
        let pipeline = GraphicsPipeline::create((layout, &modules), context)?;
        let mesh_pack = MeshPack::create((cube, allocator), context)?;
        Ok(Skybox {
            cubemap: DropGuard::new(cubemap),
            mesh_pack: DropGuard::new(mesh_pack),
            descriptor: DropGuard::new(descriptor),
            pipeline: DropGuard::new(pipeline),
        })
    }
}

impl<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> Destroy for Skybox<L> {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.descriptor.destroy(context.device.deref())?;
        self.mesh_pack.destroy(context)?;
        self.cubemap.destroy(context)?;
        self.pipeline.destroy(context.device.deref())?;
        Ok(())
    }
}
