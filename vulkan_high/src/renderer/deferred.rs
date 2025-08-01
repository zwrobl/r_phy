mod commands;
mod draw_graph;
mod presets;

use std::{cell::RefCell, convert::Infallible, error::Error, path::Path, rc::Rc, sync::Once};

use ash::vk;

use commands::Commands;
use draw_graph::DrawGraph;

use graphics::{
    model::{CommonVertex, Drawable, Mesh, MeshBuilder},
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
                    AttachmentReferences, AttachmentsBuilder, FramebufferBuilder, InputAttachment,
                },
                image::{Image, Image2D, ImagePartial},
                pipeline::{
                    GraphicsPipeline, GraphicsPipelineConfig, GraphicsPipelineListBuilder,
                    GraphicsPipelinePackList, ModuleLoader, Modules, ShaderDirectory,
                },
                render_pass::{RenderPass, Subpass},
                swapchain::Swapchain,
                ResourceIndex, ResourceIndexListBuilder,
            },
            Partial,
        },
    },
    error::{ResourceError, ShaderResult, VkError},
    Context,
};

use math::types::{Matrix4, Vector3};

use crate::{
    frame::{CameraUniformPartial, Frame, FrameContext, FrameData, FramePool},
    renderer::deferred::presets::{
        AttachmentsGBuffer, DeferedRenderPass, GBufferDepthPrepasPipeline, GBufferDescriptorSet,
        GBufferShadingPass, GBufferShadingPassPipeline, GBufferSkyboxPipeline, GBufferWritePass,
        PipelineLayoutMaterial, StatesDepthWriteDisabled,
    },
    resources::{MaterialPackList, MeshPack, MeshPackList, MeshPackPartial, Skybox, SkyboxPartial},
};

fn get_deferred_renderer_meshes() -> &'static [Mesh<CommonVertex>] {
    static mut QUAD: Option<[Mesh<CommonVertex>; 1]> = None;
    static INIT: Once = Once::new();
    unsafe {
        INIT.call_once(|| {
            if QUAD.is_none() {
                QUAD.replace([MeshBuilder::plane_subdivided(
                    0,
                    2.0 * Vector3::y(),
                    2.0 * Vector3::x(),
                    Vector3::zero(),
                    false,
                )
                .offset(Vector3::new(-1.0, -1.0, 0.0))
                .build()]);
            }
        });
        QUAD.as_ref().unwrap()
    }
}

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
    pub combined: ImagePartial<Image2D, DeviceLocal>,
    pub albedo: ImagePartial<Image2D, DeviceLocal>,
    pub normal: ImagePartial<Image2D, DeviceLocal>,
    pub position: ImagePartial<Image2D, DeviceLocal>,
    pub depth: ImagePartial<Image2D, DeviceLocal>,
}

pub struct DeferredRendererPartial {
    g_buffer: GBufferPartial,
    skybox: SkyboxPartial,
    meshes: MeshPackPartial<'static, CommonVertex>,
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
    mesh: DropGuard<MeshPack<CommonVertex>>,
    skybox: DropGuard<Skybox<GBufferSkyboxPipeline<AttachmentsGBuffer>>>,
}

pub struct DeferredRendererContext<P: GraphicsPipelinePackList> {
    renderer: Rc<RefCell<DropGuard<DeferredRenderer>>>,
    pipelines: DeferredRendererPipelines<P>,
    frames: FramePool<Self>,
    current_frame: Option<FrameData<Self>>,
}

pub struct DeferredRendererFrameState<P: GraphicsPipelinePackList> {
    commands: Commands<P>,
    draw_graph: DrawGraph,
}

pub struct DeferredRenderer {
    render_pass: RenderPass<DeferedRenderPass<AttachmentsGBuffer>>,
    frame_data: DropGuard<DeferredRendererFrameData>,
    resources: DropGuard<DeferredRendererResources>,
}

impl Frame for Rc<RefCell<DropGuard<DeferredRenderer>>> {
    type Shader<S: ShaderType> = DeferredShader<S>;
    type Context<P: GraphicsPipelinePackList> = DeferredRendererContext<P>;
    type Partial = CameraUniformPartial;

    fn load_context<'a, P: GraphicsPipelinePackList>(
        &self,
        context: &Context,
        allocator: AllocatorIndex,
        partial: Self::Partial,
        pipelines: &impl GraphicsPipelineListBuilder<Pack = P>,
    ) -> CreateResult<Self::Context<P>> {
        let renderer = self.clone();
        let pipelines = pipelines.build(context)?;
        DeferredRendererContext::create((renderer, pipelines, partial, allocator), context)
    }

    fn get_num_frames(&self) -> usize {
        self.borrow().frame_data.num_frames
    }
}

impl<P: GraphicsPipelinePackList> FrameContext for DeferredRendererContext<P> {
    const REQUIRED_COMMANDS: usize = P::LEN + 3;
    type RenderPass = DeferedRenderPass<AttachmentsGBuffer>;
    type State = DeferredRendererFrameState<P>;

    fn begin_frame(
        &mut self,
        context: &Context,
        camera_matrices: &CameraMatrices,
    ) -> Result<(), Box<dyn Error>> {
        let (primary_command, swapchain_frame, camera_descriptor) = {
            let index_list = ResourceIndexListBuilder::new()
                .push(self.renderer.borrow().frame_data.swapchain)
                .push(self.frames.camera_uniform.uniform_buffer)
                .push(self.frames.camera_uniform.descriptors)
                .push(self.frames.primary_commands)
                .build();
            context.operate_mut(
                index_list,
                |unpack_list![
                    primary_commands,
                    descriptors,
                    camera_uniform,
                    swapchain,
                    _rest
                ]| {
                    let (index, primary_command) = primary_commands.next();
                    // Here begin_primary_command is required to be caled before swapchain get_frame,
                    // as begin_command waits for the fence associated with the command execution
                    // if the order is reversed, the acquire_next_image will get the semaphore which may have operation still pending
                    // this violates the Vulkan spec
                    // TODO: Try come up with a pattern that enforces correct order of operations
                    let primary_command = context.begin_primary_command(primary_command)?;
                    let frame = context.get_frame(&***swapchain, self.frames.image_sync[index])?;
                    let descriptor = descriptors.get(index);
                    camera_uniform[index] = *camera_matrices;
                    Result::<_, Box<dyn Error>>::Ok((primary_command, frame, descriptor))
                },
            )??
        };
        let commands = self.prepare_commands(
            context,
            &swapchain_frame,
            camera_descriptor,
            camera_matrices,
        )?;
        let draw_graph = DrawGraph::new();
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
        let renderer = self.renderer.borrow();
        let index_list = ResourceIndexListBuilder::new()
            .push(renderer.frame_data.swapchain)
            .build();
        context.operate_ref(index_list, |unpack_list![swapchain, _rest]| {
            context.present_frame(&***swapchain, primary_command, swapchain_frame)
        })??;
        Ok(())
    }
}
impl Create for GBufferPartial {
    type Config<'a> = ();

    type CreateError = ResourceError;

    fn create<'a, 'b>(_config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        Ok(GBufferPartial {
            combined: context.prepare_color_attachment_image()?,
            albedo: context.prepare_color_attachment_image()?,
            normal: context.prepare_color_attachment_image()?,
            position: context.prepare_color_attachment_image()?,
            depth: context.prepare_depth_stencil_attachment_image()?,
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

impl GBuffer {
    pub fn get_framebuffer_builder(
        &self,
        context: &Context,
        extent: vk::Extent2D,
        swapchain_image: vk::ImageView,
    ) -> FramebufferBuilder<DeferedRenderPass<AttachmentsGBuffer>> {
        let index_list = ResourceIndexListBuilder::new()
            .push(self.combined)
            .push(self.albedo)
            .push(self.normal)
            .push(self.position)
            .push(self.depth)
            .build();
        context
            .operate_ref(
                index_list,
                |unpack_list![depth, position, normal, albedo, combined, _rest]| {
                    let builder = FramebufferBuilder::new(
                        extent,
                        AttachmentsBuilder::new()
                            .push(swapchain_image)
                            .push(depth.get_image_view().get_vk_image_view())
                            .push(position.get_image_view().get_vk_image_view())
                            .push(normal.get_image_view().get_vk_image_view())
                            .push(albedo.get_image_view().get_vk_image_view())
                            .push(combined.get_image_view().get_vk_image_view()),
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
        let framebuffer_builder = |swapchain_image, extent| {
            g_buffer.get_framebuffer_builder(context, extent, swapchain_image)
        };
        let swapchain = context
            .create_resource::<Swapchain<DeferedRenderPass<AttachmentsGBuffer>>, _>(
                &framebuffer_builder,
            )?;
        let (framebuffer_index, num_frames) = {
            let index_list = ResourceIndexListBuilder::new().push(swapchain).build();
            context
                .operate_ref(index_list, |unpack_list![swapchain, _rest]| {
                    let swapchain: &Swapchain<DeferedRenderPass<AttachmentsGBuffer>> =
                        &***swapchain;
                    Result::<_, Infallible>::Ok((
                        swapchain.get_framebuffer_index(0),
                        swapchain.num_images,
                    ))
                })?
                .unwrap()
        };
        let descriptors = {
            let index_list = ResourceIndexListBuilder::new()
                .push(framebuffer_index)
                .build();
            context.operate_ref(index_list, |unpack_list![framebuffer, _rest]| {
                context.create_resource::<DescriptorPool<_>, _>(
                    DescriptorSetWriter::<GBufferDescriptorSet>::new(1)
                        .write_images::<InputAttachment>(
                            &GBufferShadingPass::<AttachmentsGBuffer>::references()
                                .get_input_attachments(&*framebuffer)
                                .iter()
                                .map(|attachment| attachment.into())
                                .collect::<Vec<_>>(),
                        ),
                )
            })??
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
    type Config<'a> = (
        SkyboxPartial,
        MeshPackPartial<'static, CommonVertex>,
        AllocatorIndex,
    );
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (skybox_partial, mesh_partial, allocator) = config;
        let skybox = Skybox::create((skybox_partial, allocator), context)?;
        let mesh = MeshPack::create((mesh_partial, allocator), context)?;
        Ok(DeferredRendererResources {
            mesh: DropGuard::new(mesh),
            skybox: DropGuard::new(skybox),
        })
    }
}

impl Destroy for DeferredRendererResources {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.mesh.destroy(context)?;
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
            "_resources/shaders/spv/deferred/depth_prepass",
        )) as &dyn ModuleLoader)?;
        let shading_pass = context.create_resource(&ShaderDirectory::new(Path::new(
            "_resources/shaders/spv/deferred/gbuffer_combine",
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
            meshes: MeshPackPartial::create(get_deferred_renderer_meshes(), &context)?,
        })
    }
}

impl Partial for DeferredRendererPartial {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.g_buffer.register_memory_requirements(builder);
        self.meshes.register_memory_requirements(builder);
        self.skybox.register_memory_requirements(builder);
    }
}

impl Destroy for DeferredRendererPartial {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.g_buffer.destroy(context);
        let _ = self.meshes.destroy(context);
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
        let (
            DeferredRendererPartial {
                g_buffer,
                skybox,
                meshes,
            },
            allocator,
        ) = config;
        let render_pass = context.get_or_create_unique_resource()?;
        let frame_data = DeferredRendererFrameData::create((g_buffer, allocator), context)?;
        let resources = DeferredRendererResources::create((skybox, meshes, allocator), context)?;
        Ok(DeferredRenderer {
            render_pass,
            frame_data: DropGuard::new(frame_data),
            resources: DropGuard::new(resources),
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

impl<P: GraphicsPipelinePackList> Create for DeferredRendererContext<P> {
    type Config<'a> = (
        Rc<RefCell<DropGuard<DeferredRenderer>>>,
        P,
        CameraUniformPartial,
        AllocatorIndex,
    );
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (renderer, pipelines, camera_partial, allocator) = config;
        let (pipelines, frames) = {
            (
                DeferredRendererPipelines::create(pipelines, context)?,
                FramePool::create((camera_partial, allocator), context)?,
            )
        };
        Ok(DeferredRendererContext {
            renderer: renderer.clone(),
            pipelines,
            frames,
            current_frame: None,
        })
    }
}

impl<P: GraphicsPipelinePackList> Destroy for DeferredRendererContext<P> {
    type Context<'a> = &'a Context;
    type DestroyError = DropGuardError<Infallible>;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.pipelines.destroy(context)?;
        self.frames.destroy(context)?;
        Ok(())
    }
}
