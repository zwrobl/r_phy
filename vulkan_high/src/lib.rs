pub mod frame;
pub mod renderer;
pub mod resources;

use graphics::shader::Shader;
use math::types::Matrix4;
use type_kit::{Cons, Contains, Create, Destroy, DestroyResult, Marker, Nil};
use vulkan_low::Context;

use graphics::renderer::{
    camera::Camera, ContextBuilder, Renderer, RendererBuilder, RendererContext,
};
use graphics::{
    model::{Drawable, Material, MaterialHandle, Mesh, MeshHandle, Vertex},
    shader::{ShaderHandle, ShaderType},
};
use std::convert::Infallible;
use std::path::Path;
use std::{error::Error, marker::PhantomData};
use winit::window::Window;

use vulkan_low::device::raw::allocator::{AllocatorIndex, Static, StaticConfig};
use vulkan_low::device::raw::Partial;

use crate::frame::{CameraUniform, Frame, FrameContext};
use crate::renderer::deferred::{
    DeferredRenderer, DeferredRendererContext, DeferredRendererPartial,
};
use crate::renderer::RendererShader;
use crate::resources::{
    CommonResources, CommonResourcesPartial, GraphicsPipelineListBuilder, GraphicsPipelinePackList,
    MaterialPackList, MaterialPackListBuilder, MaterialPackListPartial, MeshPackList,
    MeshPackListBuilder, MeshPackListPartial,
};

#[derive(Debug, Clone, Copy)]
pub struct VulkanRendererConfig {}

#[derive(Debug, Clone, Copy, Default)]
pub struct VulkanRendererConfigBuilder {}

impl VulkanRendererConfig {
    pub fn builder() -> VulkanRendererConfigBuilder {
        VulkanRendererConfigBuilder::default()
    }
}

impl VulkanRendererConfigBuilder {
    pub fn build(self) -> Result<VulkanRendererConfig, Box<dyn Error>> {
        let config = VulkanRendererConfig {};
        Ok(config)
    }
}

#[derive(Debug)]
pub struct VulkanRendererBuilder<R: Destroy + Frame> {
    config: Option<VulkanRendererConfig>,
    _phantom: PhantomData<R>,
}

impl<R: Destroy + Frame> Default for VulkanRendererBuilder<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Destroy + Frame> VulkanRendererBuilder<R> {
    pub fn new() -> Self {
        Self {
            config: None,
            _phantom: PhantomData,
        }
    }

    pub fn with_config(mut self, config: VulkanRendererConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_renderer_type<N: Destroy + Frame>(self) -> VulkanRendererBuilder<N> {
        VulkanRendererBuilder {
            config: self.config,
            _phantom: PhantomData,
        }
    }
}

impl<R: Destroy + Frame> RendererBuilder for VulkanRendererBuilder<R> {
    type Renderer = VulkanRenderer;

    fn build(self, window: &Window) -> Result<Self::Renderer, Box<dyn Error>> {
        let renderer =
            VulkanRenderer::new(window, self.config.ok_or("Configuration not provided")?)?;
        Ok(renderer)
    }
}

pub struct VulkanRenderer {
    context: Context,
    renderer: DeferredRenderer,
    common_resources: CommonResources,
    allocator: AllocatorIndex,
    _config: VulkanRendererConfig,
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        let _ = self.context.wait_idle();
        let _ = self.common_resources.destroy(&self.context);
        let _ = self.renderer.destroy(&self.context);
        let _ = self.context.destroy_allocator(self.allocator);
    }
}

pub struct VulkanResourcePack<M: MaterialPackList, V: MeshPackList> {
    materials: M,
    meshes: V,
}

struct VulkanResourcePackPartial<
    'a,
    M: MaterialPackList,
    V: MeshPackList,
    PM: MaterialPackListPartial<Pack = M>,
    PV: MeshPackListPartial<Pack = V>,
> where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    materials: PM,
    meshes: PV,
    _phantom: PhantomData<&'a ()>,
}

impl<
        'a,
        M: MaterialPackList,
        V: MeshPackList,
        PM: MaterialPackListPartial<Pack = M>,
        PV: MeshPackListPartial<Pack = V>,
    > VulkanResourcePackPartial<'a, M, V, PM, PV>
where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    fn load(
        self,
        context: &Context,
        allocator: AllocatorIndex,
    ) -> Result<VulkanResourcePack<M, V>, Box<dyn Error>> {
        let Self {
            materials, meshes, ..
        } = self;
        let meshes = meshes.allocate(context, allocator)?;
        let materials = materials.allocate(context, allocator)?;
        Ok(VulkanResourcePack { materials, meshes })
    }
}

impl<
        'a,
        M: MaterialPackList,
        V: MeshPackList,
        PM: MaterialPackListPartial<Pack = M>,
        PV: MeshPackListPartial<Pack = V>,
    > Destroy for VulkanResourcePackPartial<'a, M, V, PM, PV>
where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    type Context<'b> = &'b Context;
    type DestroyError = Infallible;
    fn destroy<'b>(&mut self, context: Self::Context<'b>) -> DestroyResult<Self> {
        let _ = self.materials.destroy(context);
        let _ = self.meshes.destroy(context);
        Ok(())
    }
}
impl<
        'a,
        M: MaterialPackList,
        V: MeshPackList,
        PM: MaterialPackListPartial<Pack = M>,
        PV: MeshPackListPartial<Pack = V>,
    > Partial for VulkanResourcePackPartial<'a, M, V, PM, PV>
where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    fn register_memory_requirements<B: vulkan_low::device::raw::allocator::AllocatorBuilder>(
        &self,
        builder: &mut B,
    ) {
        self.materials.register_memory_requirements(builder);
        self.meshes.register_memory_requirements(builder);
    }
}

type PackPartial<'a, M, V, MB, MV> = VulkanResourcePackPartial<
    'a,
    M,
    V,
    <MB as MaterialPackListBuilder>::Partial<'a>,
    <MV as MeshPackListBuilder>::Partial<'a>,
>;

impl<M: MaterialPackList, V: MeshPackList> VulkanResourcePack<M, V> {
    fn prepare<'a, MB: MaterialPackListBuilder<Pack = M>, MV: MeshPackListBuilder<Pack = V>>(
        context: &Context,
        materials: &'a MB,
        meshes: &'a MV,
    ) -> Result<PackPartial<'a, M, V, MB, MV>, Box<dyn Error>> {
        let materials = materials.prepare(context)?;
        let meshes = meshes.prepare(context)?;
        Ok(VulkanResourcePackPartial {
            materials,
            meshes,
            _phantom: PhantomData,
        })
    }
}

impl<M: MaterialPackList, V: MeshPackList> Destroy for VulkanResourcePack<M, V> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.materials.destroy(context);
        let _ = self.meshes.destroy(context);
        Ok(())
    }
}

pub struct VulkanRendererContext<
    'a,
    M: MaterialPackList,
    V: MeshPackList,
    P: GraphicsPipelinePackList,
> {
    renderer: &'a VulkanRenderer,
    renderer_context: DeferredRendererContext<'a, P>,
    resources: VulkanResourcePack<M, V>,
    allocator: AllocatorIndex,
}

impl VulkanRenderer {
    pub fn new(window: &Window, config: VulkanRendererConfig) -> Result<Self, Box<dyn Error>> {
        let context = Context::build(window)?;
        let common_resources = CommonResourcesPartial::create((), &context)?;
        let renderer_partial = DeferredRendererPartial::create(
            Path::new("_resources/assets/skybox/skybox"),
            &context,
        )?;
        let mut allocator_config = StaticConfig::new();
        common_resources.register_memory_requirements(&mut allocator_config);
        renderer_partial.register_memory_requirements(&mut allocator_config);
        let allocator = context.create_allocator::<Static>(allocator_config)?;
        let common_meshes = CommonResources::create((common_resources, allocator), &context)?;
        let renderer = DeferredRenderer::create((renderer_partial, allocator), &context)?;
        Ok(Self {
            context,
            renderer,
            common_resources: common_meshes,
            allocator,
            _config: config,
        })
    }
}

impl<'a, M: MaterialPackList, V: MeshPackList, S: GraphicsPipelinePackList> Drop
    for VulkanRendererContext<'a, M, V, S>
{
    fn drop(&mut self) {
        let _ = self.renderer.context.wait_idle();
        let _ = self.resources.destroy(&self.renderer.context);
        let _ = self.renderer_context.destroy(&self.renderer.context);
        let _ = self.renderer.context.destroy_allocator(self.allocator);
    }
}

impl Renderer for VulkanRenderer {}

#[derive(Debug)]
pub struct VulkanContextBuilder<
    P: GraphicsPipelineListBuilder,
    M: MaterialPackListBuilder,
    V: MeshPackListBuilder,
> {
    shaders: P,
    materials: M,
    meshes: V,
}

impl<P: GraphicsPipelineListBuilder, M: MaterialPackListBuilder, V: MeshPackListBuilder>
    ContextBuilder for VulkanContextBuilder<P, M, V>
{
    type Renderer = VulkanRenderer;
    type Context<'a> = VulkanRendererContext<'a, M::Pack, V::Pack, P::Pack>;

    fn build<'a>(self, renderer: &'a Self::Renderer) -> Result<Self::Context<'a>, Box<dyn Error>> {
        let mut config = StaticConfig::new();
        let camera_uniform = CameraUniform::prepare(&renderer.context, &renderer.renderer)?;
        let resources =
            VulkanResourcePack::prepare(&renderer.context, &self.materials, &self.meshes)?;
        camera_uniform.register_memory_requirements(&mut config);
        resources.register_memory_requirements(&mut config);
        let allocator = renderer.context.create_allocator::<Static>(config)?;
        let renderer_context = renderer.renderer.load_context(
            &renderer.context,
            allocator,
            camera_uniform,
            &self.shaders,
        )?;
        let resources = resources.load(&renderer.context, allocator)?;
        Ok(VulkanRendererContext {
            renderer,
            renderer_context,
            resources,
            allocator,
        })
    }
}

impl Default for VulkanContextBuilder<Nil, Nil, Nil> {
    fn default() -> Self {
        Self::new()
    }
}

impl VulkanContextBuilder<Nil, Nil, Nil> {
    pub fn new() -> Self {
        VulkanContextBuilder {
            shaders: Nil::new(),
            materials: Nil::new(),
            meshes: Nil::new(),
        }
    }
}

fn push_and_get_index<V>(vec: &mut Vec<V>, value: V) -> u32 {
    let index = vec.len();
    vec.push(value);
    index.try_into().unwrap()
}

type ShaderCollection<V, M> = Vec<RendererShader<DeferredRenderer, V, M>>;

impl<S: GraphicsPipelineListBuilder, M: MaterialPackListBuilder, V: MeshPackListBuilder>
    VulkanContextBuilder<S, M, V>
{
    pub fn with_material_type<N: Material>(self) -> VulkanContextBuilder<S, Cons<Vec<N>, M>, V> {
        VulkanContextBuilder {
            materials: Cons {
                head: vec![],
                tail: self.materials,
            },
            meshes: self.meshes,
            shaders: self.shaders,
        }
    }

    pub fn with_mesh_type<N: Vertex>(self) -> VulkanContextBuilder<S, M, Cons<Vec<Mesh<N>>, V>> {
        VulkanContextBuilder {
            meshes: Cons {
                head: vec![],
                tail: self.meshes,
            },
            materials: self.materials,
            shaders: self.shaders,
        }
    }

    pub fn with_shader_type<N: Vertex, T: Material>(
        self,
    ) -> VulkanContextBuilder<Cons<ShaderCollection<N, T>, S>, M, V> {
        VulkanContextBuilder {
            shaders: Cons {
                head: vec![],
                tail: self.shaders,
            },
            materials: self.materials,
            meshes: self.meshes,
        }
    }

    pub fn add_material<N: Material, T: Marker>(&mut self, material: N) -> MaterialHandle<N>
    where
        M: Contains<Vec<N>, T>,
    {
        MaterialHandle::new(push_and_get_index(self.materials.get_mut(), material))
    }

    pub fn add_mesh<N: Vertex, T: Marker>(&mut self, mesh: Mesh<N>) -> MeshHandle<N>
    where
        V: Contains<Vec<Mesh<N>>, T>,
    {
        MeshHandle::new(push_and_get_index(self.meshes.get_mut(), mesh))
    }

    pub fn add_shader<N: Vertex, T: Material, K: Marker>(
        &mut self,
        shader: Shader<N, T>,
    ) -> ShaderHandle<Shader<N, T>>
    where
        S: Contains<ShaderCollection<N, T>, K>,
    {
        ShaderHandle::new(push_and_get_index(self.shaders.get_mut(), shader.into()))
    }
}

impl<
        'a,
        M: MaterialPackList + 'static,
        V: MeshPackList + 'static,
        S: GraphicsPipelinePackList + 'static,
    > RendererContext for VulkanRendererContext<'a, M, V, S>
{
    type Renderer = VulkanRenderer;
    type Shaders = S;
    type Materials = M;
    type Meshes = V;

    fn begin_frame<C: Camera>(&mut self, camera: &C) -> Result<(), Box<dyn Error>> {
        let camera_matrices = camera.get_matrices();
        self.renderer_context.begin_frame(
            &self.renderer.context,
            &self.renderer.common_resources,
            &camera_matrices,
        )?;
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Box<dyn Error>> {
        self.renderer_context.end_frame(&self.renderer.context)?;
        Ok(())
    }

    fn draw<T: ShaderType, D: Drawable<Material = T::Material, Vertex = T::Vertex>>(
        &mut self,
        shader: ShaderHandle<T>,
        drawable: &D,
        transform: &Matrix4,
    ) -> Result<(), Box<dyn Error>> {
        self.renderer_context.draw(
            &self.renderer.context,
            shader,
            drawable,
            transform,
            &self.resources.materials,
            &self.resources.meshes,
        );
        Ok(())
    }
}
