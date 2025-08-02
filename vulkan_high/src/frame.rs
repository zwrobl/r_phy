// Temporary allow for too_many_arguments
// handling render commands will be significantly revamped in the future
// which makes it not worth the effort to refactor this code
#![allow(clippy::too_many_arguments)]

use std::{convert::Infallible, error::Error, marker::PhantomData};

use type_kit::{
    unpack_list, Cons, Create, CreateCollection, CreateResult, Destroy, DestroyCollection,
    DestroyResult, DropGuard, DropGuardError,
};

use graphics::{
    model::Drawable,
    renderer::camera::CameraMatrices,
    shader::{ShaderHandle, ShaderType},
};
use math::types::Matrix4;
use vulkan_low::{
    device::raw::{
        allocator::{AllocatorBuilder, AllocatorIndex},
        resources::{
            buffer::{UniformBuffer, UniformBufferInfoBuilder, UniformBufferPartial},
            layout::presets::CameraDescriptorSet,
            pipeline::{
                GraphicsPipelineConfig, GraphicsPipelineListBuilder, GraphicsPipelinePackList,
                ModuleLoader,
            },
            render_pass::RenderPassConfig,
            ResourceIndex,
        },
        Partial,
    },
    error::{ResourceError, VkError, VkResult},
    index_list, Context,
};

use vulkan_low::device::{
    raw::resources::descriptor::{Descriptor, DescriptorPool, DescriptorSetWriter},
    raw::resources::swapchain::{SwapchainFrame, SwapchainImageSync},
    raw::resources::{
        command::{
            level::{Primary, Secondary},
            operation::Graphics,
            BeginCommand, Persistent, PersistentCommandPool,
        },
        ResourceIndexListBuilder,
    },
};

use crate::resources::{MaterialPackList, MeshPackList};

pub trait Frame: 'static {
    type Shader<S: ShaderType>: ShaderType + GraphicsPipelineConfig + ModuleLoader;
    type Context<P: GraphicsPipelinePackList>: FrameContext
        + for<'a> Create<Context<'a> = &'a Context>;
    type Partial;

    fn load_context<'a, P: GraphicsPipelinePackList>(
        &self,
        context: &Context,
        allocator: AllocatorIndex,
        partial: Self::Partial,
        pipelines: &impl GraphicsPipelineListBuilder<Pack = P>,
    ) -> CreateResult<Self::Context<P>>;

    fn get_num_frames(&self) -> usize;
}

pub trait FrameContext: Sized {
    const REQUIRED_COMMANDS: usize;
    type RenderPass: RenderPassConfig;
    type State;

    fn begin_frame(
        &mut self,
        device: &Context,
        camera: &CameraMatrices,
    ) -> Result<(), Box<dyn Error>>;

    fn draw<
        S: ShaderType,
        D: Drawable<Material = S::Material, Vertex = S::Vertex>,
        M: MaterialPackList,
        V: MeshPackList,
    >(
        &mut self,
        context: &Context,
        shader: ShaderHandle<S>,
        drawable: &D,
        transform: &Matrix4,
        material_packs: &M,
        mesh_packs: &V,
    );

    fn end_frame(&mut self, device: &Context) -> Result<(), Box<dyn Error>>;
}

pub struct CameraUniformPartial {
    buffer: DropGuard<UniformBufferPartial<CameraMatrices, Graphics>>,
}

impl Destroy for CameraUniformPartial {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.buffer.destroy(context);
        Ok(())
    }
}

impl Partial for CameraUniformPartial {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.buffer.register_memory_requirements(builder);
    }
}

impl CameraUniform {
    #[inline]
    pub fn prepare<R: Frame>(context: &Context, renderer: &R) -> VkResult<CameraUniformPartial> {
        let partial = UniformBufferPartial::create(
            UniformBufferInfoBuilder::new().with_len(renderer.get_num_frames()),
            &context,
        )?;
        Ok(CameraUniformPartial {
            buffer: DropGuard::new(partial),
        })
    }
}

pub struct CameraUniform {
    pub descriptors: ResourceIndex<DescriptorPool<CameraDescriptorSet>>,
    pub uniform_buffer: ResourceIndex<UniformBuffer<CameraMatrices, Graphics>>,
    pub len: usize,
}

pub struct FrameData<C: FrameContext> {
    pub swapchain_frame: SwapchainFrame<C::RenderPass>,
    pub primary_command: BeginCommand<Persistent, Primary, Graphics>,
    pub camera_descriptor: Descriptor<CameraDescriptorSet>,
    pub renderer_state: C::State,
}

pub struct FramePool<F: FrameContext> {
    pub image_sync: Vec<SwapchainImageSync>,
    pub camera_uniform: CameraUniform,
    pub primary_commands: ResourceIndex<PersistentCommandPool<Primary, Graphics>>,
    pub secondary_commands: ResourceIndex<PersistentCommandPool<Secondary, Graphics>>,
    _phantom: PhantomData<F>,
}

impl Create for CameraUniform {
    type Config<'a> = (CameraUniformPartial, AllocatorIndex);
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (CameraUniformPartial { buffer }, allocator) = config;
        let uniform_buffer =
            context.create_resource::<UniformBuffer<_, _>, _>((buffer, allocator))?;
        let (descriptors, len) = context.operate_ref(
            index_list![uniform_buffer],
            |unpack_list![uniform_buffer]| {
                let len = uniform_buffer.len();
                let descriptors = context.create_resource(
                    DescriptorSetWriter::<CameraDescriptorSet>::new(len)
                        .write_buffer(&uniform_buffer),
                )?;
                Result::<_, ResourceError>::Ok((descriptors, len))
            },
        )??;
        Ok(CameraUniform {
            descriptors,
            uniform_buffer,
            len,
        })
    }
}

impl Destroy for CameraUniform {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = context.destroy_resource(self.descriptors);
        let _ = context.destroy_resource(self.uniform_buffer);
        Ok(())
    }
}

impl<F: FrameContext> Create for FramePool<F> {
    type Config<'a> = <CameraUniform as Create>::Config<'a>;
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let camera_uniform = CameraUniform::create(config, context)?;
        let frame_count = camera_uniform.len;
        let image_sync = (0..camera_uniform.len)
            .map(|_| ())
            .create(context)
            .collect::<Result<Vec<_>, _>>()?;
        let primary_commands = context.create_resource(frame_count)?;
        let secondary_commands = context.create_resource(frame_count * F::REQUIRED_COMMANDS)?;
        Ok(FramePool {
            image_sync,
            camera_uniform,
            primary_commands,
            secondary_commands,
            _phantom: PhantomData,
        })
    }
}

impl<F: FrameContext> Destroy for FramePool<F> {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.image_sync.iter_mut().destroy(context)?;
        self.camera_uniform.destroy(context)?;
        let _ = context.destroy_resource(self.primary_commands);
        let _ = context.destroy_resource(self.secondary_commands);
        Ok(())
    }
}
