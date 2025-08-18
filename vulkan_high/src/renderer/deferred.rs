pub mod presets;
mod resources;
mod stage;

use std::{
    convert::Infallible,
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
};

use graphics::{renderer::camera::CameraMatrices, shader::ShaderType};
use type_kit::{
    Cons, Create, Destroy, DropGuard, Executor, SynchronousExecutor, TypedNil, list_type,
    list_value, unpack_list,
};
use vulkan_low::{
    Context, index_list,
    memory::allocator::{AllocatorBuilder, AllocatorIndex},
    resources::{
        Partial, ResourceIndex,
        command::{Graphics, PersistentCommandPool, Secondary},
        descriptor::{Descriptor, DescriptorSetMapper},
        error::{ResourceError, ResourceResult, ShaderResult},
        layout::presets::CameraDescriptorSet,
        pipeline::{GraphicsPipelineConfig, ModuleLoader, Modules, ShaderDirectory},
        storage::ResourceIndexListBuilder,
    },
};

use crate::{
    VulkanContext,
    renderer::{
        DestroyTerminator, ExternalResources, Renderer, RendererBuilder, RendererContext,
        ResourceCell, ShaderDescriptor,
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
    },
    resources::{GraphicsPipelinePackList, SkyboxPartial},
};

pub type DeferredFrameData = list_type![
    ResourceCell<PersistentCommandPool<Secondary, Graphics>>,
    FrameCell<DeferedRenderPass<AttachmentsGBuffer>>,
    DrawStorage,
    TypedNil<DestroyTerminator>
];

pub struct DeferredRenderer<
    E: Executor<InitializerList = DeferredFrameData, TaskError = ResourceError>,
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
}

pub struct DeferredRendererPartial {
    g_buffer_partial: GBufferPartial,
    skybox_partial: SkyboxPartial,
    frame_pool_partial: DropGuard<FramePoolPartial<DeferedRenderPass<AttachmentsGBuffer>>>,
}

impl Partial for DeferredRendererPartial {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.g_buffer_partial.register_memory_requirements(builder);
        self.skybox_partial.register_memory_requirements(builder);
        self.frame_pool_partial
            .register_memory_requirements(builder);
    }
}

impl Create for DeferredRendererPartial {
    type Config<'a> = DeferredRendererConfig;
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let DeferredRendererConfig { skybox } = config;
        let g_buffer_partial = GBufferPartial::create((), context)?;
        let frame_pool_partial = DropGuard::new(FramePoolPartial::create((), context)?);
        let skybox_partial = SkyboxPartial::create(&skybox, context)?;
        Ok(Self {
            g_buffer_partial,
            skybox_partial,
            frame_pool_partial,
        })
    }
}

impl Destroy for DeferredRendererPartial {
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

impl<E: Executor<InitializerList = DeferredFrameData, TaskError = ResourceError>> Destroy
    for DeferredRenderer<E>
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

pub struct DeferredRendererContext<
    'b,
    E: Executor<
            InitializerList = DeferredFrameData,
            TaskError = ResourceError,
            TaskResult = Frame<DeferedRenderPass<AttachmentsGBuffer>>,
        > + 'static,
    P: GraphicsPipelinePackList,
> where
    for<'a> E::Resources: Destroy<Context<'a> = &'a Context>,
    <E::Resources as Destroy>::DestroyError: Into<Infallible>,
    for<'a> E::TaskList: Destroy<Context<'a> = &'a Context>,
    <E::TaskList as Destroy>::DestroyError: Into<Infallible>,
{
    renderer: &'b mut DeferredRenderer<E>,
    command_pool: ResourceIndex<PersistentCommandPool<Secondary, Graphics>>,
    _pipelines: PhantomData<P>,
}

impl<
    'b,
    E: Executor<
            InitializerList = DeferredFrameData,
            TaskError = ResourceError,
            TaskResult = Frame<DeferedRenderPass<AttachmentsGBuffer>>,
        > + 'static,
    P: GraphicsPipelinePackList,
> Destroy for DeferredRendererContext<'b, E, P>
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
        let _ = context.destroy_resource(self.command_pool);
        Ok(())
    }
}

impl<
    'b,
    E: Executor<
            InitializerList = DeferredFrameData,
            TaskError = ResourceError,
            TaskResult = Frame<DeferedRenderPass<AttachmentsGBuffer>>,
        > + 'static,
    P: GraphicsPipelinePackList,
> RendererContext for DeferredRendererContext<'b, E, P>
where
    for<'a> E::Resources: Destroy<Context<'a> = &'a Context>,
    <E::Resources as Destroy>::DestroyError: Into<Infallible>,
    for<'a> E::TaskList: Destroy<Context<'a> = &'a Context>,
    <E::TaskList as Destroy>::DestroyError: Into<Infallible>,
{
    fn begin_frame(
        &mut self,
        context: &Context,
        camera: CameraMatrices,
    ) -> ResourceResult<Descriptor<CameraDescriptorSet>> {
        self.renderer.frame = self.renderer.frame_pool.acquire(context, &camera)?;
        Ok(self.renderer.frame.camera_descriptor)
    }

    fn render(&mut self, context: &Context, draw_calls: DrawStorage) -> ResourceResult<()> {
        let frame = self
            .renderer
            .executor
            .as_mut()
            .unwrap()
            .execute(list_value![
                ResourceCell::new(self.command_pool),
                self.renderer.frame.take(),
                draw_calls,
                TypedNil::<DestroyTerminator>::new()
            ])?;
        self.renderer.frame_pool.present(context, frame)?;
        Ok(())
    }
}

impl<
    E: Executor<
            InitializerList = DeferredFrameData,
            TaskError = ResourceError,
            TaskResult = Frame<DeferedRenderPass<AttachmentsGBuffer>>,
        > + 'static,
> Renderer for DeferredRenderer<E>
where
    for<'a> E::Resources: Destroy<Context<'a> = &'a Context>,
    <E::Resources as Destroy>::DestroyError: Into<Infallible>,
    for<'a> E::TaskList: Destroy<Context<'a> = &'a Context>,
    <E::TaskList as Destroy>::DestroyError: Into<Infallible>,
{
    type ShaderType<T: ShaderType> = DeferredShader<T>;
    type RendererContext<'b, P: GraphicsPipelinePackList> = DeferredRendererContext<'b, E, P>;

    fn load_context<'a, P: GraphicsPipelinePackList>(
        &'a mut self,
        context: &Context,
    ) -> ResourceResult<Self::RendererContext<'a, P>> {
        let command_pool = context
            .create_resource::<PersistentCommandPool<Secondary, Graphics>, _>(
                self.frame_pool.num_images() * (P::LEN + 4),
            )?;
        let renderer_context = DeferredRendererContext {
            renderer: self,
            command_pool,
            _pipelines: PhantomData::<P>,
        };
        Ok(renderer_context)
    }
}

pub struct DeferredRendererBuilder {
    context: Rc<VulkanContext>,
    allocator: Option<AllocatorIndex>,
    partial: DeferredRendererPartial,
}

impl Partial for DeferredRendererBuilder {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.partial.register_memory_requirements(builder);
    }
}

impl Destroy for DeferredRendererBuilder {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, context: &Context) -> Result<(), Self::DestroyError> {
        let _ = self.partial.destroy(context);
        Ok(())
    }
}

pub struct DeferredRendererConfig {
    skybox: PathBuf,
}

impl DeferredRendererConfig {
    #[inline]
    pub fn new(skybox: &Path) -> Self {
        Self {
            skybox: skybox.to_path_buf(),
        }
    }
}

impl RendererBuilder for DeferredRendererBuilder {
    type Config = DeferredRendererConfig;

    #[inline]
    fn new(context: &Rc<VulkanContext>, config: Self::Config) -> ResourceResult<Self> {
        let partial = DeferredRendererPartial::create(config, context)?;
        Ok(DeferredRendererBuilder {
            context: context.clone(),
            allocator: None,
            partial,
        })
    }

    #[inline]
    fn with_allocator<T: Into<AllocatorIndex>>(self, allocator: T) -> Self {
        Self {
            allocator: Some(allocator.into()),
            ..self
        }
    }

    fn build(self) -> ResourceResult<impl Renderer> {
        let Self {
            context,
            allocator,
            partial:
                DeferredRendererPartial {
                    g_buffer_partial,
                    skybox_partial,
                    frame_pool_partial,
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
            .register_resource(DeferredSharedResources::create(framebuffer, &context)?)
            .register_resource(ResourceCell::empty())
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
        };
        Ok(new_deferred)
    }
}

pub struct DeferredShader<S: ShaderType> {
    shader: S,
}

impl<T: ShaderType> ShaderDescriptor<CameraDescriptorSet> for DeferredShader<T> {
    #[inline]
    fn get_mapper() -> DescriptorSetMapper<CameraDescriptorSet, Self::Layout> {
        Descriptor::<CameraDescriptorSet>::get_mapper::<Self, _>()
    }
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
