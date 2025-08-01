use std::{convert::Infallible, path::Path, sync::Once};

use graphics::{
    model::{CommonVertex, Mesh},
    renderer::camera::CameraMatrices,
};
use math::types::Vector4;
use physics::shape;

use type_kit::{unpack_list, Cons, Create, Destroy, DestroyResult, DropGuard, DropGuardError, Nil};
use vulkan_low::{
    device::raw::{
        allocator::{AllocatorBuilder, AllocatorIndex},
        resources::{
            command::{level::Level, operation::Operation, RecordingCommand},
            descriptor::{DescriptorPool, DescriptorSetWriter},
            image::{Image2D, ImageCube, ImageCubeReader, Texture, TexturePartial},
            layout::{presets::TextureDescriptorSet, PipelineLayoutBuilder},
            pipeline::{GraphicsPipeline, GraphicsPipelineConfig, ModuleLoader, ShaderDirectory},
            ResourceIndex, ResourceIndexListBuilder,
        },
        Partial,
    },
    error::ResourceError,
    Context,
};

use crate::resources::{bind_mesh_pack, MeshPack, MeshPackPartial};

pub type LayoutSkybox =
    PipelineLayoutBuilder<Cons<TextureDescriptorSet, Nil>, Cons<CameraMatrices, Nil>>;

pub struct SkyboxPartial {
    cubemap: TexturePartial<ImageCube, ImageCubeReader>,
    cube: MeshPackPartial<'static, CommonVertex>,
}

pub struct Skybox<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> {
    pub mesh_pack: DropGuard<MeshPack<CommonVertex>>,
    cubemap: ResourceIndex<Texture<ImageCube>>,
    pub descriptor: ResourceIndex<DescriptorPool<TextureDescriptorSet>>,
    pub pipeline: ResourceIndex<GraphicsPipeline<L>>,
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

impl Create for SkyboxPartial {
    type Config<'a> = &'a Path;

    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        Ok(SkyboxPartial {
            cubemap: TexturePartial::create(ImageCubeReader::new(config)?, context)?,
            cube: MeshPackPartial::create(get_skybox_meshes(), context)?,
        })
    }
}

impl Partial for SkyboxPartial {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.cube.register_memory_requirements(builder);
        self.cubemap.register_memory_requirements(builder);
    }
}

impl Destroy for SkyboxPartial {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.cube.destroy(context);
        let _ = self.cubemap.destroy(context);
        Ok(())
    }
}

impl<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> Create for Skybox<L> {
    type Config<'a> = (SkyboxPartial, AllocatorIndex);
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (SkyboxPartial { cubemap, cube }, allocator) = config;
        let cubemap = context.create_resource::<Texture<_>, _>((cubemap, allocator))?;
        let index_list = ResourceIndexListBuilder::new().push(cubemap).build();
        let descriptor =
            context.operate_ref(index_list, |unpack_list![cubemap, _allocator]| {
                let image_info = (&***cubemap).into();
                context.create_resource(
                    DescriptorSetWriter::<TextureDescriptorSet>::new(1)
                        .write_images::<Texture<Image2D>>(&[image_info]),
                )
            })??;
        let modules = ShaderDirectory::new(Path::new(SKYBOX_SHADER));
        let pipeline = context.create_resource(&modules as &dyn ModuleLoader)?;
        let mesh_pack = MeshPack::create((cube, allocator), context)?;
        Ok(Skybox {
            mesh_pack: DropGuard::new(mesh_pack),
            cubemap,
            descriptor,
            pipeline,
        })
    }
}

impl<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> Destroy for Skybox<L> {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.mesh_pack.destroy(context)?;
        let _ = context.destroy_resource(self.descriptor);
        let _ = context.destroy_resource(self.cubemap);
        let _ = context.destroy_resource(self.pipeline);
        Ok(())
    }
}

pub fn draw_skybox<
    'a,
    T,
    L: Level,
    O: Operation,
    C: GraphicsPipelineConfig<Layout = LayoutSkybox>,
>(
    context: &Context,
    skybox: &Skybox<C>,
    command: RecordingCommand<'a, T, L, O>,
    mut camera_matrices: CameraMatrices,
) -> RecordingCommand<'a, T, L, O> {
    camera_matrices.view[3] = Vector4::w();
    let index_list = ResourceIndexListBuilder::new()
        .push(skybox.pipeline)
        .push(skybox.descriptor)
        .build();
    context
        .operate_ref(index_list, |unpack_list![descriptor, pipeline, _rest]| {
            let command = bind_mesh_pack(
                context,
                command
                    .bind_pipeline(&***pipeline)
                    .bind_descriptor_set(&descriptor.get(0).get_binding_data(&pipeline).unwrap())
                    .push_constants(pipeline.get_push_range(&camera_matrices)),
                &*skybox.mesh_pack,
            )
            .draw_indexed(skybox.mesh_pack.get(0));
            Result::<_, Infallible>::Ok(command)
        })
        .unwrap()
        .unwrap()
}
