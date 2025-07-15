// Temporary allow for too_many_arguments
// handling render commands will be significantly revamped in the future
// which makes it not worth the effort to refactor this code
#![allow(clippy::too_many_arguments)]

use std::{convert::Infallible, error::Error, marker::PhantomData, ops::Deref};

use type_kit::{
    Create, CreateCollection, CreateResult, Destroy, DestroyCollection, DestroyResult, DropGuard,
    DropGuardError,
};

use crate::context::{
    device::raw::{
        allocator::AllocatorIndex,
        resources::buffer::{UniformBuffer, UniformBufferInfoBuilder, UniformBufferPartial},
        Partial,
    },
    error::{VkError, VkResult},
    Context,
};
use graphics::{
    model::Drawable,
    renderer::camera::CameraMatrices,
    shader::{ShaderHandle, ShaderType},
};
use math::types::Matrix4;

use super::{
    command::{
        level::{Primary, Secondary},
        operation::Graphics,
        BeginCommand, Persistent, PersistentCommandPool,
    },
    descriptor::{CameraDescriptorSet, Descriptor, DescriptorPool, DescriptorSetWriter},
    framebuffer::AttachmentList,
    pipeline::{
        GraphicsPipelineConfig, GraphicsPipelineListBuilder, GraphicsPipelinePackList, ModuleLoader,
    },
    resources::{MaterialPackList, MeshPackList},
    swapchain::{SwapchainFrame, SwapchainImageSync},
    Device,
};

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
    type Attachments: AttachmentList;
    type State;

    fn begin_frame(
        &mut self,
        device: &Device,
        camera: &CameraMatrices,
    ) -> Result<(), Box<dyn Error>>;

    fn draw<
        S: ShaderType,
        D: Drawable<Material = S::Material, Vertex = S::Vertex>,
        M: MaterialPackList,
        V: MeshPackList,
    >(
        &mut self,
        shader: ShaderHandle<S>,
        drawable: &D,
        transform: &Matrix4,
        material_packs: &M,
        mesh_packs: &V,
    );

    fn end_frame(&mut self, device: &Device) -> Result<(), Box<dyn Error>>;
}

pub struct CameraUniformPartial {
    buffer: UniformBufferPartial<CameraMatrices, Graphics>,
}

impl Destroy for CameraUniformPartial {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.buffer.destroy(context)
    }
}

impl Partial for CameraUniformPartial {
    fn register_memory_requirements<B: super::raw::allocator::AllocatorBuilder>(
        &self,
        builder: &mut B,
    ) {
        self.buffer.register_memory_requirements(builder);
    }
}

impl CameraUniform {
    #[inline]
    pub fn prepare<R: Frame>(context: &Context, renderer: &R) -> VkResult<CameraUniformPartial> {
        Ok(CameraUniformPartial {
            buffer: UniformBufferPartial::create(
                UniformBufferInfoBuilder::new().with_len(renderer.get_num_frames()),
                &context,
            )?,
        })
    }
}

pub struct CameraUniform {
    pub descriptors: DropGuard<DescriptorPool<CameraDescriptorSet>>,
    pub uniform_buffer: DropGuard<UniformBuffer<CameraMatrices, Graphics>>,
}

pub struct FrameData<C: FrameContext> {
    pub swapchain_frame: SwapchainFrame<C::Attachments>,
    pub primary_command: BeginCommand<Persistent, Primary, Graphics>,
    pub camera_descriptor: Descriptor<CameraDescriptorSet>,
    pub renderer_state: C::State,
}

pub struct FramePool<F: FrameContext> {
    pub image_sync: Vec<SwapchainImageSync>,
    pub camera_uniform: CameraUniform,
    pub primary_commands: PersistentCommandPool<Primary, Graphics>,
    pub secondary_commands: PersistentCommandPool<Secondary, Graphics>,
    _phantom: PhantomData<F>,
}

impl Create for CameraUniform {
    type Config<'a> = (CameraUniformPartial, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (CameraUniformPartial { buffer }, allocator) = config;
        let uniform_buffer = UniformBuffer::create((buffer, allocator), context)?;
        let descriptors = DescriptorPool::create(
            DescriptorSetWriter::<CameraDescriptorSet>::new(uniform_buffer.len())
                .write_buffer(&uniform_buffer),
            context,
        )?;
        Ok(CameraUniform {
            descriptors: DropGuard::new(descriptors),
            uniform_buffer: DropGuard::new(uniform_buffer),
        })
    }
}

impl Destroy for CameraUniform {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.descriptors.destroy(context);
        let _ = self.uniform_buffer.destroy(context);
        Ok(())
    }
}

impl<F: FrameContext> Create for FramePool<F> {
    type Config<'a> = <CameraUniform as Create>::Config<'a>;
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let camera_uniform = CameraUniform::create(config, context)?;
        let frame_count = camera_uniform.descriptors.len();
        let image_sync = (0..camera_uniform.descriptors.len())
            .map(|_| ())
            .create(context.device.deref())
            .collect::<Result<Vec<_>, _>>()?;
        let primary_commands = PersistentCommandPool::create(frame_count, context)?;
        let secondary_commands =
            PersistentCommandPool::create(frame_count * F::REQUIRED_COMMANDS, context)?;

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
        self.primary_commands.destroy(context)?;
        self.secondary_commands.destroy(context)?;
        self.camera_uniform.destroy(context)?;
        Ok(())
    }
}
