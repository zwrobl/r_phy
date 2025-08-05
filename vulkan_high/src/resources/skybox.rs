use std::{convert::Infallible, path::Path};

use bytemuck::Zeroable;
use graphics::renderer::camera::CameraMatrices;

use math::types::Vector4;
use type_kit::{unpack_list, Cons, Create, Destroy, DestroyResult, DropGuard, DropGuardError, Nil};
use vulkan_low::{
    index_list,
    memory::allocator::{AllocatorBuilder, AllocatorIndex},
    resources::{
        command::{BindPipeline, Level, Lifetime, Operation, Recorder, RecordingCommand},
        descriptor::{DescriptorBindingData, DescriptorPool, DescriptorSetWriter},
        error::ResourceError,
        image::{Image2D, ImageCube, ImageCubeReader, Texture, TexturePartial},
        layout::{presets::TextureDescriptorSet, PipelineLayoutBuilder},
        pipeline::{
            GraphicsPipeline, GraphicsPipelineConfig, ModuleLoader, PushConstantData,
            ShaderDirectory,
        },
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

struct SkyboxBindings {
    pipeline: BindPipeline,
    descriptor: DescriptorBindingData,
    camera: PushConstantData<CameraMatrices>,
}

pub struct Skybox<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> {
    bindings: SkyboxBindings,
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
                context.create_resource::<DescriptorPool<_>, _>(
                    DescriptorSetWriter::<TextureDescriptorSet>::new(1)
                        .write_images::<Texture<Image2D>>(&[image_info]),
                )
            })??;
        let modules = ShaderDirectory::new(Path::new(SKYBOX_SHADER));
        let pipeline =
            context.create_resource::<GraphicsPipeline<_>, _>(&modules as &dyn ModuleLoader)?;
        let bindings = context.operate_ref(
            index_list![pipeline, descriptor],
            |unpack_list![descriptor, pipeline]| SkyboxBindings {
                pipeline: pipeline.bind(),
                descriptor: descriptor.get(0).get_binding(pipeline),
                camera: pipeline.map(CameraMatrices::zeroed()),
            },
        )?;
        Ok(Skybox {
            bindings,
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

pub struct DrawSkybox<'a, C: GraphicsPipelineConfig<Layout = LayoutSkybox>> {
    skybox: &'a Skybox<C>,
    resources: &'a CommonResources,
    camera: CameraMatrices,
}

impl<L: GraphicsPipelineConfig<Layout = LayoutSkybox>> Skybox<L> {
    #[inline]
    pub fn draw<'a>(
        &'a self,
        resources: &'a CommonResources,
        mut camera: CameraMatrices,
    ) -> DrawSkybox<'a, L> {
        camera.view[3] = Vector4::w();
        DrawSkybox {
            skybox: self,
            resources,
            camera,
        }
    }
}

impl<C: GraphicsPipelineConfig<Layout = LayoutSkybox>> Recorder for DrawSkybox<'_, C> {
    #[inline]
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let camera = self.skybox.bindings.camera.with_data(self.camera);
        command
            .push(&self.skybox.bindings.pipeline)
            .push(&self.skybox.bindings.descriptor)
            // TODO: Consider alternative approach to mapping push constants
            .push(&camera)
            .push(&self.resources.draw(CommonMesh::Cube))
    }
}
