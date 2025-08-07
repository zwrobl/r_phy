pub mod presets;
mod resources;
mod stage;

use std::{convert::Infallible, marker::PhantomData, path::Path, rc::Rc};

use graphics::{renderer::camera::CameraMatrices, shader::ShaderType};
use type_kit::{
    list_value, unpack_list, Cons, Create, Destroy, DropGuard, Executor, SynchronousExecutor,
    TypedNil,
};
use vulkan_low::{
    index_list,
    memory::allocator::{AllocatorBuilder, AllocatorIndex},
    resources::{
        descriptor::Descriptor,
        error::{ResourceError, ResourceResult, ShaderResult},
        layout::presets::CameraDescriptorSet,
        pipeline::{GraphicsPipelineConfig, ModuleLoader, Modules, ShaderDirectory},
        storage::ResourceIndexListBuilder,
        Partial,
    },
    Context,
};

use crate::{
    renderer::{
        deferred::{
            presets::{
                AttachmentsGBuffer, DeferedRenderPass, GBufferWritePass, PipelineLayoutMaterial,
                StatesDepthWriteDisabled,
            },
            resources::{DeferredSharedResources, GBuffer, GBufferPartial},
            stage::{
                depth_prepass::DepthPrepass, draw_skybox::DrawSkybox,
                gbuffer_combine::GBufferCombine, gbuffer_write::GBufferWrite,
                load_resources::LoadResources,
            },
        },
        frame::{Frame, FrameCell, FramePool, FramePoolPartial},
        storage::DrawStorage,
        DestroyTerminator, ExternalResources, FrameData, Renderer, RendererBuilder,
    },
    resources::{GraphicsPipelinePackList, SkyboxPartial},
    VulkanContext,
};

pub type DeferredFrameData = FrameData<DeferedRenderPass<AttachmentsGBuffer>>;

pub struct DeferredRenderer<
    E: Executor<InitializerList = DeferredFrameData, TaskError = ResourceError>,
    P: GraphicsPipelinePackList,
> where
    for<'a> E::Resources: Destroy<Context<'a> = &'a Context>,
    <E::Resources as Destroy>::DestroyError: Into<Infallible>,
    for<'a> E::TaskList: Destroy<Context<'a> = &'a Context>,
    <E::TaskList as Destroy>::DestroyError: Into<Infallible>,
{
    executor: Option<E>,
    g_buffer: GBuffer,
    frame: FrameCell<DeferedRenderPass<AttachmentsGBuffer>>,
    frame_pool: FramePool<DeferedRenderPass<AttachmentsGBuffer>>,
    _phantom: PhantomData<P>,
}

pub struct DeferredRendererPartial<P: GraphicsPipelinePackList> {
    g_buffer_partial: GBufferPartial,
    skybox_partial: SkyboxPartial,
    frame_pool_partial: DropGuard<FramePoolPartial<DeferedRenderPass<AttachmentsGBuffer>>>,
    _phantom: PhantomData<P>,
}

impl<P: GraphicsPipelinePackList> Partial for DeferredRendererPartial<P> {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.g_buffer_partial.register_memory_requirements(builder);
        self.skybox_partial.register_memory_requirements(builder);
        self.frame_pool_partial
            .register_memory_requirements(builder);
    }
}

impl<P: GraphicsPipelinePackList> Create for DeferredRendererPartial<P> {
    type Config<'a> = &'a Path;
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let g_buffer_partial = GBufferPartial::create((), context)?;
        let frame_pool_partial = DropGuard::new(FramePoolPartial::create((), context)?);
        let skybox_partial = SkyboxPartial::create(config, context)?;
        Ok(Self {
            g_buffer_partial,
            skybox_partial,
            frame_pool_partial,
            _phantom: PhantomData,
        })
    }
}

impl<P: GraphicsPipelinePackList> Destroy for DeferredRendererPartial<P> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, context: &Context) -> Result<(), Self::DestroyError> {
        let _ = self.g_buffer_partial.destroy(context);
        let _ = self.skybox_partial.destroy(context);
        let _ = self.frame_pool_partial.destroy(context);
        Ok(())
    }
}

impl<
        E: Executor<InitializerList = DeferredFrameData, TaskError = ResourceError>,
        P: GraphicsPipelinePackList,
    > Destroy for DeferredRenderer<E, P>
where
    for<'a> E::Resources: Destroy<Context<'a> = &'a Context>,
    <E::Resources as Destroy>::DestroyError: Into<Infallible>,
    for<'a> E::TaskList: Destroy<Context<'a> = &'a Context>,
    <E::TaskList as Destroy>::DestroyError: Into<Infallible>,
{
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, context: &Context) -> Result<(), Self::DestroyError> {
        let (mut resources, mut tasks) = self.executor.take().unwrap().into_inner();
        let _ = resources.destroy(context);
        let _ = tasks.destroy(context);
        let _ = self.g_buffer.destroy(context);
        let _ = self.frame_pool.destroy(context);
        Ok(())
    }
}

impl<
        E: Executor<
            InitializerList = DeferredFrameData,
            TaskError = ResourceError,
            TaskResult = Frame<DeferedRenderPass<AttachmentsGBuffer>>,
        >,
        P: GraphicsPipelinePackList,
    > Renderer for DeferredRenderer<E, P>
where
    for<'a> E::Resources: Destroy<Context<'a> = &'a Context>,
    <E::Resources as Destroy>::DestroyError: Into<Infallible>,
    for<'a> E::TaskList: Destroy<Context<'a> = &'a Context>,
    <E::TaskList as Destroy>::DestroyError: Into<Infallible>,
{
    fn destroy(&mut self, context: &Context) {
        let _ = <Self as Destroy>::destroy(self, context);
    }

    fn begin_frame(
        &mut self,
        context: &Context,
        camera: CameraMatrices,
    ) -> ResourceResult<Descriptor<CameraDescriptorSet>> {
        self.frame = self.frame_pool.acquire(context, &camera)?;
        Ok(self.frame.camera_descriptor)
    }

    #[inline]
    fn render(&mut self, context: &Context, draw_calls: DrawStorage) -> ResourceResult<()> {
        let frame = self.executor.as_mut().unwrap().execute(list_value![
            self.frame.take(),
            draw_calls,
            TypedNil::<DestroyTerminator>::new()
        ])?;
        let _ = self.frame_pool.present(context, frame)?;
        Ok(())
    }
}

pub struct DeferredRendererBuilder<P: GraphicsPipelinePackList> {
    context: Rc<VulkanContext>,
    allocator: Option<AllocatorIndex>,
    partial: DeferredRendererPartial<P>,
}

impl<P: GraphicsPipelinePackList> Partial for DeferredRendererBuilder<P> {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.partial.register_memory_requirements(builder);
    }
}

impl<P: GraphicsPipelinePackList> Destroy for DeferredRendererBuilder<P> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, context: &Context) -> Result<(), Self::DestroyError> {
        let _ = self.partial.destroy(context);
        Ok(())
    }
}

impl<P: GraphicsPipelinePackList> RendererBuilder for DeferredRendererBuilder<P> {
    type Config<'a> = &'a Path;

    #[inline]
    fn new(context: Rc<VulkanContext>, config: Self::Config<'_>) -> ResourceResult<Self> {
        let skybox_path = config;
        let partial = DeferredRendererPartial::create(skybox_path, &context)?;
        Ok(DeferredRendererBuilder {
            context,
            allocator: None,
            partial,
        })
    }

    #[inline]
    fn with_allocator(self, allocator: AllocatorIndex) -> Self {
        Self {
            allocator: Some(allocator),
            ..self
        }
    }

    fn build<'a>(self) -> ResourceResult<impl Renderer> {
        let Self {
            context,
            allocator,
            partial:
                DeferredRendererPartial {
                    g_buffer_partial,
                    skybox_partial,
                    frame_pool_partial,
                    ..
                },
        } = self;
        let g_buffer = GBuffer::create((g_buffer_partial, allocator), &context)?;
        let frame_pool = FramePool::create((&g_buffer, frame_pool_partial, allocator), &context)?;
        let framebuffer = context.operate_ref(
            index_list![frame_pool.swapchain],
            |unpack_list![swapchain]| swapchain.get_framebuffer_index(0),
        )?;
        let executor = SynchronousExecutor::builder()
            .with_resource_terminator_type::<DestroyTerminator>()
            .with_task_terminator_type::<DestroyTerminator>()
            .register_resource(ExternalResources::new(&context))
            .register_resource(FrameCell::<DeferedRenderPass<AttachmentsGBuffer>>::empty())
            .register_resource(DrawStorage::new())
            .register_resource(DeferredSharedResources::create(
                (framebuffer, frame_pool.num_images() * (P::LEN + 4)),
                &context,
            )?)
            .push_task(LoadResources)
            .push_task(DepthPrepass::create((), &context)?)
            .push_task(DrawSkybox::create((skybox_partial, allocator), &context)?)
            .push_task(GBufferWrite)
            .push_task(GBufferCombine::create((), &context)?)
            .build();
        let new_deferred = DeferredRenderer {
            executor: Some(executor),
            g_buffer,
            frame: FrameCell::empty(),
            frame_pool,
            _phantom: PhantomData::<P>,
        };
        Ok(new_deferred)
    }
}

pub struct DeferredShader<S: ShaderType> {
    shader: S,
}

impl<S: ShaderType> ShaderType for DeferredShader<S> {
    type Material = S::Material;
    type Vertex = S::Vertex;

    #[inline]
    fn source(&self) -> &Path {
        self.shader.source()
    }
}

impl<S: ShaderType> GraphicsPipelineConfig for DeferredShader<S> {
    type Attachments = AttachmentsGBuffer;
    type Layout = PipelineLayoutMaterial<S::Material>;
    type PipelineStates = StatesDepthWriteDisabled<S::Vertex>;
    type RenderPass = DeferedRenderPass<AttachmentsGBuffer>;
    type Subpass = GBufferWritePass<AttachmentsGBuffer>;
}

impl<S: ShaderType> From<S> for DeferredShader<S> {
    #[inline]
    fn from(shader: S) -> Self {
        DeferredShader { shader }
    }
}

impl<S: ShaderType> ModuleLoader for DeferredShader<S> {
    #[inline]
    fn load<'a>(&self, context: &'a Context) -> ShaderResult<Modules<'a>> {
        ShaderDirectory::new(self.shader.source()).load(context)
    }
}
