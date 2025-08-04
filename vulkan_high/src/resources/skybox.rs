use std::{convert::Infallible, path::Path};

use graphics::renderer::camera::CameraMatrices;
use math::types::Vector4;

use type_kit::{unpack_list, Cons, Create, Destroy, DestroyResult, DropGuard, DropGuardError, Nil};
use vulkan_low::{
    index_list,
    memory::allocator::{AllocatorBuilder, AllocatorIndex},
    resources::{
        command::{level::Level, operation::Operation, RecordingCommand},
        descriptor::{DescriptorPool, DescriptorSetWriter},
        error::ResourceError,
        image::{Image2D, ImageCube, ImageCubeReader, Texture, TexturePartial},
        layout::{presets::TextureDescriptorSet, PipelineLayoutBuilder},
        pipeline::{GraphicsPipeline, GraphicsPipelineConfig, ModuleLoader, ShaderDirectory},
        storage::ResourceIndexListBuilder,
        Partial, ResourceIndex,
    },
    Context,
};

use crate::resources::{CommonMesh, CommonResources};

pub type LayoutSkybox =
    PipelineLayoutBuilder<Cons<TextureDescriptorSet, Nil>, Cons<CameraMatrices, Nil>>;

pub struct SkyboxPartial {
    cubemap: DropGuard<TexturePartial<ImageCube, ImageCubeReader>>,
}

pub struct Skybox<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> {
    cubemap: ResourceIndex<Texture<ImageCube>>,
    pub descriptor: ResourceIndex<DescriptorPool<TextureDescriptorSet>>,
    pub pipeline: ResourceIndex<GraphicsPipeline<L>>,
}

const SKYBOX_SHADER: &str = "_resources/shaders/spv/skybox";

impl Create for SkyboxPartial {
    type Config<'a> = &'a Path;

    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        Ok(SkyboxPartial {
            cubemap: DropGuard::new(TexturePartial::create(
                ImageCubeReader::new(config)?,
                context,
            )?),
        })
    }
}

impl Partial for SkyboxPartial {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.cubemap.register_memory_requirements(builder);
    }
}

impl Destroy for SkyboxPartial {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.cubemap.destroy(context);
        Ok(())
    }
}

impl<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> Create for Skybox<L> {
    type Config<'a> = (SkyboxPartial, Option<AllocatorIndex>);
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (SkyboxPartial { cubemap }, allocator) = config;
        let cubemap = context.create_resource::<Texture<_>, _>((cubemap, allocator))?;
        let descriptor =
            context.operate_ref(index_list![cubemap], |unpack_list![cubemap]| {
                let image_info = cubemap.into();
                context.create_resource(
                    DescriptorSetWriter::<TextureDescriptorSet>::new(1)
                        .write_images::<Texture<Image2D>>(&[image_info]),
                )
            })??;
        let modules = ShaderDirectory::new(Path::new(SKYBOX_SHADER));
        let pipeline = context.create_resource(&modules as &dyn ModuleLoader)?;
        Ok(Skybox {
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
    common_meshes: &CommonResources,
    command: RecordingCommand<'a, T, L, O>,
    mut camera_matrices: CameraMatrices,
) -> RecordingCommand<'a, T, L, O> {
    camera_matrices.view[3] = Vector4::w();
    context
        .operate_ref(
            index_list![skybox.pipeline, skybox.descriptor],
            |unpack_list![descriptor, pipeline]| {
                let command = command
                    .bind_pipeline(pipeline.get_binding_data())
                    .bind_descriptor_set(&descriptor.get(0).get_binding_data(pipeline).unwrap())
                    .push_constants(pipeline.get_push_range(&camera_matrices));
                common_meshes.draw(context, command, CommonMesh::Cube)
            },
        )
        .unwrap()
}
