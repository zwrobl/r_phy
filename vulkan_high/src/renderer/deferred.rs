mod presets;
mod storage;

use std::{convert::Infallible, error::Error, path::Path};

use storage::{CommandStorage, DrawStorage};

use graphics::{
    model::Drawable,
    renderer::camera::CameraMatrices,
    shader::{ShaderHandle, ShaderType},
};

use type_kit::{
    unpack_list, Cons, Create, CreateResult, Destroy, DestroyResult, DropGuard, DropGuardError,
};

use vulkan_low::{
    device::{
        memory::DeviceLocal,
        raw::{
            allocator::{AllocatorBuilder, AllocatorIndex},
            resources::{
                descriptor::{DescriptorPool, DescriptorSetWriter},
                framebuffer::{
                    AttachmentReferences, AttachmentsBuilder, Extent2D, FramebufferBuilder,
                    InputAttachment,
                },
                image::{Image, Image2D, ImagePartial, ImageView},
                pipeline::{
                    GraphicsPipeline, GraphicsPipelineConfig, ModuleLoader, Modules,
                    ShaderDirectory,
                },
                render_pass::{RenderPass, Subpass},
                swapchain::{Swapchain, SwapchainFramebufferConfigBuilder},
                ResourceIndex, ResourceIndexListBuilder,
            },
            Partial,
        },
    },
    error::{ResourceError, ShaderResult, VkError},
    index_list, Context,
};

use math::types::Matrix4;

use crate::{
    frame::{CameraUniformPartial, Frame, FrameContext, FrameData, FramePool},
    renderer::deferred::presets::{
        AttachmentsGBuffer, DeferedRenderPass, GBufferDepthPrepasPipeline, GBufferDescriptorSet,
        GBufferShadingPass, GBufferShadingPassPipeline, GBufferSkyboxPipeline, GBufferWritePass,
        PipelineLayoutMaterial, StatesDepthWriteDisabled,
    },
    resources::{
        CommonResources, GraphicsPipelineListBuilder, GraphicsPipelinePackList, MaterialPackList,
        MeshPackList, Skybox, SkyboxPartial,
    },
};

const DEPTH_PREPASS_SHADER: &str = "_resources/shaders/spv/deferred/depth_prepass";
const GBUFFER_COMBINE_SHADER: &str = "_resources/shaders/spv/deferred/gbuffer_combine";

pub struct DeferredShader<S: ShaderType> {
    shader: S,
}

impl<S: ShaderType> ShaderType for DeferredShader<S> {
    type Material = S::Material;
    type Vertex = S::Vertex;

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
    fn from(shader: S) -> Self {
        DeferredShader { shader }
    }
}

impl<S: ShaderType> ModuleLoader for DeferredShader<S> {
    fn load<'a>(&self, context: &'a Context) -> ShaderResult<Modules<'a>> {
        ShaderDirectory::new(self.shader.source()).load(context)
    }
}

pub struct GBufferPartial {
    pub combined: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
    pub albedo: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
    pub normal: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
    pub position: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
    pub depth: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
}

pub struct DeferredRendererPartial {
    g_buffer: GBufferPartial,
    skybox: SkyboxPartial,
}

pub struct GBuffer {
    pub combined: ResourceIndex<Image<Image2D, DeviceLocal>>,
    pub albedo: ResourceIndex<Image<Image2D, DeviceLocal>>,
    pub normal: ResourceIndex<Image<Image2D, DeviceLocal>>,
    pub position: ResourceIndex<Image<Image2D, DeviceLocal>>,
    pub depth: ResourceIndex<Image<Image2D, DeviceLocal>>,
}

struct DeferredRendererPipelines<P: GraphicsPipelinePackList> {
    write_pass: P,
    depth_prepass: ResourceIndex<GraphicsPipeline<GBufferDepthPrepasPipeline<AttachmentsGBuffer>>>,
    shading_pass: ResourceIndex<GraphicsPipeline<GBufferShadingPassPipeline<AttachmentsGBuffer>>>,
}

struct DeferredRendererFrameData {
    num_frames: usize,
    g_buffer: DropGuard<GBuffer>,
    swapchain: ResourceIndex<Swapchain<DeferedRenderPass<AttachmentsGBuffer>>>,
    descriptors: ResourceIndex<DescriptorPool<GBufferDescriptorSet>>,
}

struct DeferredRendererResources {
    skybox: DropGuard<Skybox<GBufferSkyboxPipeline<AttachmentsGBuffer>>>,
}

pub struct DeferredRendererContext<'a, P: GraphicsPipelinePackList> {
    renderer: &'a DeferredRenderer,
    pipelines: DeferredRendererPipelines<P>,
    frames: FramePool<Self>,
    current_frame: Option<FrameData<Self>>,
}

pub struct DeferredRendererFrameState<P: GraphicsPipelinePackList> {
    commands: CommandStorage<P>,
    draw_graph: DrawStorage,
}

pub struct DeferredRenderer {
    render_pass: RenderPass<DeferedRenderPass<AttachmentsGBuffer>>,
    frame_data: DeferredRendererFrameData,
    resources: DeferredRendererResources,
}

impl Frame for DeferredRenderer {
    type Shader<S: ShaderType> = DeferredShader<S>;
    type Context<'a, P: GraphicsPipelinePackList> = DeferredRendererContext<'a, P>;
    type Partial = CameraUniformPartial;

    fn load_context<'a, P: GraphicsPipelinePackList>(
        &'a self,
        context: &'a Context,
        allocator: AllocatorIndex,
        partial: Self::Partial,
        pipelines: &impl GraphicsPipelineListBuilder<Pack = P>,
    ) -> Result<Self::Context<'a, P>, Box<dyn Error>> {
        let pipelines = pipelines.build(context)?;
        let (pipelines, frames) = {
            (
                DeferredRendererPipelines::create(pipelines, context)?,
                FramePool::create((partial, allocator), context)?,
            )
        };
        Ok(DeferredRendererContext {
            renderer: self,
            pipelines,
            frames,
            current_frame: None,
        })
    }

    fn get_num_frames(&self) -> usize {
        self.frame_data.num_frames
    }
}

impl<'a, P: GraphicsPipelinePackList> FrameContext for DeferredRendererContext<'a, P> {
    const REQUIRED_COMMANDS: usize = P::LEN + 3;
    type RenderPass = DeferedRenderPass<AttachmentsGBuffer>;
    type State = DeferredRendererFrameState<P>;

    fn begin_frame(
        &mut self,
        context: &Context,
        common_resources: &CommonResources,
        camera_matrices: &CameraMatrices,
    ) -> Result<(), Box<dyn Error>> {
        let (primary_command, swapchain_frame, camera_descriptor) = {
            context.operate_mut(
                index_list![
                    self.renderer.frame_data.swapchain,
                    self.frames.camera_uniform.uniform_buffer,
                    self.frames.camera_uniform.descriptors,
                    self.frames.primary_commands
                ],
                |unpack_list![primary_commands, descriptors, camera_uniform, swapchain]| {
                    let (index, primary_command) = primary_commands.next_command();
                    // Here begin_primary_command is required to be caled before swapchain get_frame,
                    // as begin_command waits for the fence associated with the command execution
                    // if the order is reversed, the acquire_next_image will get the semaphore which may have operation still pending
                    // this violates the Vulkan spec
                    // TODO: Try come up with a pattern that enforces correct order of operations
                    let primary_command = context.begin_primary_command(primary_command)?;
                    let frame = context.get_frame(swapchain, self.frames.image_sync[index])?;
                    let descriptor = descriptors.get(index);
                    camera_uniform[index] = *camera_matrices;
                    Result::<_, Box<dyn Error>>::Ok((primary_command, frame, descriptor))
                },
            )??
        };
        let commands = self.prepare_commands(
            context,
            common_resources,
            &swapchain_frame,
            camera_descriptor,
            camera_matrices,
        )?;
        let draw_graph = DrawStorage::new();
        self.current_frame.replace(FrameData {
            swapchain_frame,
            primary_command,
            camera_descriptor,
            renderer_state: DeferredRendererFrameState {
                commands,
                draw_graph,
            },
        });
        Ok(())
    }

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
    ) {
        self.append_draw_call(
            context,
            material_packs,
            mesh_packs,
            shader,
            drawable,
            transform,
        );
    }

    fn end_frame(&mut self, context: &Context) -> Result<(), Box<dyn Error>> {
        let FrameData {
            swapchain_frame,
            primary_command,
            renderer_state,
            ..
        } = self.current_frame.take().ok_or("current_frame is None!")?;
        let commands = self.record_draw_calls(context, renderer_state, &swapchain_frame)?;
        let primary_command =
            self.record_primary_command(context, primary_command, commands, &swapchain_frame)?;
        let renderer = self.renderer;
        context.operate_ref(
            index_list![renderer.frame_data.swapchain],
            |unpack_list![swapchain]| {
                context.present_frame(swapchain, primary_command, swapchain_frame)
            },
        )??;
        Ok(())
    }
}
impl Create for GBufferPartial {
    type Config<'a> = ();

    type CreateError = ResourceError;

    fn create<'a, 'b>(_config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        Ok(GBufferPartial {
            combined: DropGuard::new(context.prepare_color_attachment_image()?),
            albedo: DropGuard::new(context.prepare_color_attachment_image()?),
            normal: DropGuard::new(context.prepare_color_attachment_image()?),
            position: DropGuard::new(context.prepare_color_attachment_image()?),
            depth: DropGuard::new(context.prepare_depth_stencil_attachment_image()?),
        })
    }
}

impl Partial for GBufferPartial {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.albedo.register_memory_requirements(builder);
        self.combined.register_memory_requirements(builder);
        self.depth.register_memory_requirements(builder);
        self.normal.register_memory_requirements(builder);
        self.position.register_memory_requirements(builder);
    }
}

impl Destroy for GBufferPartial {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.albedo.destroy(context);
        let _ = self.combined.destroy(context);
        let _ = self.depth.destroy(context);
        let _ = self.normal.destroy(context);
        let _ = self.position.destroy(context);
        Ok(())
    }
}

impl SwapchainFramebufferConfigBuilder<DeferedRenderPass<AttachmentsGBuffer>> for GBuffer {
    fn get_framebuffer_builder(
        &self,
        context: &Context,
        swapchain_image: &ImageView<Image2D>,
        extent: Extent2D,
    ) -> FramebufferBuilder<DeferedRenderPass<AttachmentsGBuffer>> {
        context
            .operate_ref(
                index_list![
                    self.combined,
                    self.albedo,
                    self.normal,
                    self.position,
                    self.depth
                ],
                |unpack_list![depth, position, normal, albedo, combined]| {
                    let builder = FramebufferBuilder::new(
                        extent,
                        AttachmentsBuilder::new()
                            .push(swapchain_image)
                            .push(depth.get_image_view())
                            .push(position.get_image_view())
                            .push(normal.get_image_view())
                            .push(albedo.get_image_view())
                            .push(combined.get_image_view()),
                    );
                    Result::<_, Infallible>::Ok(builder)
                },
            )
            .unwrap()
            .unwrap()
    }
}

impl Create for GBuffer {
    type Config<'a> = (GBufferPartial, AllocatorIndex);
    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (partial, allocator) = config;
        let combined = context.create_resource::<Image<_, _>, _>((partial.combined, allocator))?;
        let albedo = context.create_resource::<Image<_, _>, _>((partial.albedo, allocator))?;
        let normal = context.create_resource::<Image<_, _>, _>((partial.normal, allocator))?;
        let position = context.create_resource::<Image<_, _>, _>((partial.position, allocator))?;
        let depth = context.create_resource::<Image<_, _>, _>((partial.depth, allocator))?;
        Ok(GBuffer {
            combined,
            albedo,
            normal,
            position,
            depth,
        })
    }
}

impl Destroy for GBuffer {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = context.destroy_resource(self.combined);
        let _ = context.destroy_resource(self.albedo);
        let _ = context.destroy_resource(self.normal);
        let _ = context.destroy_resource(self.position);
        let _ = context.destroy_resource(self.depth);
        Ok(())
    }
}

impl Create for DeferredRendererFrameData {
    type Config<'a> = <GBuffer as Create>::Config<'a>;
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let g_buffer = GBuffer::create(config, context)?;
        let swapchain = context
            .create_resource::<Swapchain<DeferedRenderPass<AttachmentsGBuffer>>, _>(&g_buffer)?;
        let (framebuffer_index, num_frames) = {
            context
                .operate_ref(index_list![swapchain], |unpack_list![swapchain]| {
                    let swapchain: &Swapchain<DeferedRenderPass<AttachmentsGBuffer>> = swapchain;
                    Result::<_, Infallible>::Ok((
                        swapchain.get_framebuffer_index(0),
                        swapchain.num_images,
                    ))
                })?
                .unwrap()
        };
        let descriptors = {
            context.operate_ref(
                index_list![framebuffer_index],
                |unpack_list![framebuffer]| {
                    context.create_resource::<DescriptorPool<_>, _>(
                        DescriptorSetWriter::<GBufferDescriptorSet>::new(1)
                            .write_images::<InputAttachment>(
                                &GBufferShadingPass::<AttachmentsGBuffer>::references()
                                    .get_input_attachments(framebuffer)
                                    .iter()
                                    .map(|attachment| attachment.into())
                                    .collect::<Vec<_>>(),
                            ),
                    )
                },
            )??
        };
        Ok(DeferredRendererFrameData {
            g_buffer: DropGuard::new(g_buffer),
            descriptors,
            swapchain,
            num_frames,
        })
    }
}

impl Destroy for DeferredRendererFrameData {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = context.destroy_resource(self.descriptors);
        self.g_buffer.destroy(context)?;
        Ok(())
    }
}

impl Create for DeferredRendererResources {
    type Config<'a> = (SkyboxPartial, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (skybox_partial, allocator) = config;
        let skybox = Skybox::create((skybox_partial, allocator), context)?;
        Ok(DeferredRendererResources {
            skybox: DropGuard::new(skybox),
        })
    }
}

impl Destroy for DeferredRendererResources {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.skybox.destroy(context)?;
        Ok(())
    }
}

impl<P: GraphicsPipelinePackList> Create for DeferredRendererPipelines<P> {
    type Config<'a> = P;
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let depth_prepass = context.create_resource(&ShaderDirectory::new(Path::new(
            DEPTH_PREPASS_SHADER,
        )) as &dyn ModuleLoader)?;
        let shading_pass = context.create_resource(&ShaderDirectory::new(Path::new(
            GBUFFER_COMBINE_SHADER,
        )) as &dyn ModuleLoader)?;
        Ok(DeferredRendererPipelines {
            write_pass: config,
            depth_prepass,
            shading_pass,
        })
    }
}

impl<P: GraphicsPipelinePackList> Destroy for DeferredRendererPipelines<P> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.write_pass.destroy(context);
        let _ = context.destroy_resource(self.depth_prepass);
        let _ = context.destroy_resource(self.shading_pass);
        Ok(())
    }
}

impl Create for DeferredRendererPartial {
    type Config<'a> = &'a Path;

    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        Ok(DeferredRendererPartial {
            g_buffer: GBufferPartial::create((), context)?,
            skybox: SkyboxPartial::create(config, context)?,
        })
    }
}

impl Partial for DeferredRendererPartial {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.g_buffer.register_memory_requirements(builder);
        self.skybox.register_memory_requirements(builder);
    }
}

impl Destroy for DeferredRendererPartial {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.g_buffer.destroy(context);
        let _ = self.skybox.destroy(context);
        Ok(())
    }
}

impl Create for DeferredRenderer {
    type Config<'a> = (DeferredRendererPartial, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (DeferredRendererPartial { g_buffer, skybox }, allocator) = config;
        let render_pass = context.get_or_create_unique_resource()?;
        let frame_data = DeferredRendererFrameData::create((g_buffer, allocator), context)?;
        let resources = DeferredRendererResources::create((skybox, allocator), context)?;
        Ok(DeferredRenderer {
            render_pass,
            frame_data,
            resources,
        })
    }
}

impl Destroy for DeferredRenderer {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.frame_data.destroy(context)?;
        self.resources.destroy(context)?;
        Ok(())
    }
}

impl<'c, P: GraphicsPipelinePackList> Destroy for DeferredRendererContext<'c, P> {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.pipelines.destroy(context)?;
        self.frames.destroy(context)?;
        Ok(())
    }
}
