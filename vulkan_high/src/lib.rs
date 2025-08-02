pub mod frame;
pub mod renderer;
pub mod resources;

use math::types::Matrix4;
use type_kit::{Cons, Contains, Create, Destroy, DestroyResult, DropGuard, Marker, Nil};
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
use std::{cell::RefCell, error::Error, marker::PhantomData, rc::Rc};
use winit::window::Window;

use vulkan_low::device::raw::allocator::{AllocatorIndex, Static, StaticConfig};
use vulkan_low::device::raw::Partial;

use crate::frame::{CameraUniform, CameraUniformPartial, Frame, FrameContext};
use crate::renderer::deferred::{DeferredRenderer, DeferredRendererPartial};
use crate::resources::{
    GraphicsPipelineListBuilder, GraphicsPipelinePackList, MaterialPackList,
    MaterialPackListBuilder, MaterialPackListPartial, MeshPackList, MeshPackListBuilder,
    MeshPackListPartial,
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
pub struct VulkanRendererBuilder<R: Destroy>
where
    Rc<RefCell<DropGuard<R>>>: Frame,
{
    config: Option<VulkanRendererConfig>,
    _phantom: PhantomData<R>,
}

impl<R: Destroy> VulkanRendererBuilder<R>
where
    Rc<RefCell<DropGuard<R>>>: Frame,
{
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

    pub fn with_renderer_type<N: Destroy>(self) -> VulkanRendererBuilder<N>
    where
        Rc<RefCell<DropGuard<N>>>: Frame,
    {
        VulkanRendererBuilder {
            config: self.config,
            _phantom: PhantomData,
        }
    }
}

impl<R: Destroy> RendererBuilder for VulkanRendererBuilder<R>
where
    Rc<RefCell<DropGuard<R>>>: Frame,
{
    type Renderer = VulkanRenderer;

    fn build(self, window: &Window) -> Result<Self::Renderer, Box<dyn Error>> {
        let renderer =
            VulkanRenderer::new(window, self.config.ok_or("Configuration not provided")?)?;
        Ok(renderer)
    }
}

pub struct VulkanRenderer {
    context: Rc<RefCell<Context>>,
    renderer: Rc<RefCell<DropGuard<DeferredRenderer>>>,
    allocator: AllocatorIndex,
    _config: VulkanRendererConfig,
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        let context = self.context.borrow();
        let _ = context.wait_idle();
        let mut renderer = self.renderer.borrow_mut();
        let _ = renderer.destroy(&context);
        let _ = context.destroy_allocator(self.allocator);
    }
}

pub struct VulkanResourcePack<
    R: Frame,
    M: MaterialPackList,
    V: MeshPackList,
    S: GraphicsPipelinePackList,
> {
    allocator: AllocatorIndex,
    materials: M,
    meshes: V,
    renderer_context: R::Context<S>,
}

impl<
        R: Frame<Partial = CameraUniformPartial>,
        M: MaterialPackList,
        V: MeshPackList,
        S: GraphicsPipelinePackList,
    > VulkanResourcePack<R, M, V, S>
{
    fn load(
        context: &mut Context,
        renderer: &R,
        materials: &impl MaterialPackListBuilder<Pack = M>,
        meshes: &impl MeshPackListBuilder<Pack = V>,
        pipelines: &impl GraphicsPipelineListBuilder<Pack = S>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut allocator_config = StaticConfig::new();
        let materials = materials.prepare(&context)?;
        let meshes = meshes.prepare(&context)?;
        let camera_uniform = CameraUniform::prepare(context, renderer)?;
        materials.register_memory_requirements(&mut allocator_config);
        meshes.register_memory_requirements(&mut allocator_config);
        camera_uniform.register_memory_requirements(&mut allocator_config);
        let allocator = context.create_allocator::<Static>(allocator_config)?;
        let meshes = meshes.allocate(&context, allocator)?;
        let materials = materials.allocate(&context, allocator)?;
        let renderer_context =
            renderer.load_context(&context, allocator, camera_uniform, pipelines)?;
        Ok(Self {
            materials,
            meshes,
            renderer_context,
            allocator,
        })
    }
}

impl<R: Frame, M: MaterialPackList, V: MeshPackList, S: GraphicsPipelinePackList> Destroy
    for VulkanResourcePack<R, M, V, S>
{
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.materials.destroy(context);
        let _ = self.meshes.destroy(context);
        let _ = self.renderer_context.destroy(context);
        let _ = context.destroy_allocator(self.allocator);
        Ok(())
    }
}

pub struct VulkanRendererContext<
    R: Frame,
    M: MaterialPackList,
    V: MeshPackList,
    S: GraphicsPipelinePackList,
> {
    context: Rc<RefCell<Context>>,
    resources: VulkanResourcePack<R, M, V, S>,
}

impl VulkanRenderer {
    pub fn new(window: &Window, config: VulkanRendererConfig) -> Result<Self, Box<dyn Error>> {
        let cubemap_path = Path::new("_resources/assets/skybox/skybox");
        let context = Context::build(window)?;
        let renderer_partial = DeferredRendererPartial::create(cubemap_path, &context)?;
        let mut allocator_config = StaticConfig::new();
        renderer_partial.register_memory_requirements(&mut allocator_config);
        let allocator = context.create_allocator::<Static>(allocator_config)?;
        let renderer = DeferredRenderer::create((renderer_partial, allocator), &context)?;
        Ok(Self {
            context: Rc::new(RefCell::new(context)),
            renderer: Rc::new(RefCell::new(DropGuard::new(renderer))),
            allocator,
            _config: config,
        })
    }
}

impl<R: Frame, M: MaterialPackList, V: MeshPackList, S: GraphicsPipelinePackList> Drop
    for VulkanRendererContext<R, M, V, S>
{
    fn drop(&mut self) {
        let context = self.context.borrow();
        let _ = self.context.borrow().wait_idle();
        let _ = self.resources.destroy(&*context);
    }
}

impl Renderer for VulkanRenderer {}

#[derive(Debug)]
pub struct VulkanContextBuilder<
    R: Frame,
    S: GraphicsPipelineListBuilder,
    M: MaterialPackListBuilder,
    V: MeshPackListBuilder,
> {
    shaders: S,
    materials: M,
    meshes: V,
    _phantom: PhantomData<R>,
}

impl<S: GraphicsPipelineListBuilder, M: MaterialPackListBuilder, V: MeshPackListBuilder>
    ContextBuilder for VulkanContextBuilder<Rc<RefCell<DropGuard<DeferredRenderer>>>, S, M, V>
{
    type Renderer = VulkanRenderer;
    type Context =
        VulkanRendererContext<Rc<RefCell<DropGuard<DeferredRenderer>>>, M::Pack, V::Pack, S::Pack>;

    fn build(self, renderer: &Self::Renderer) -> Result<Self::Context, Box<dyn Error>> {
        let mut context = renderer.context.borrow_mut();
        let resources = VulkanResourcePack::load(
            &mut context,
            &renderer.renderer,
            &self.materials,
            &self.meshes,
            &self.shaders,
        )?;
        Ok(VulkanRendererContext {
            context: renderer.context.clone(),
            resources,
        })
    }
}

impl Default for VulkanContextBuilder<Rc<RefCell<DropGuard<DeferredRenderer>>>, Nil, Nil, Nil> {
    fn default() -> Self {
        Self::new()
    }
}

impl VulkanContextBuilder<Rc<RefCell<DropGuard<DeferredRenderer>>>, Nil, Nil, Nil> {
    pub fn new() -> Self {
        VulkanContextBuilder {
            shaders: Nil::new(),
            materials: Nil::new(),
            meshes: Nil::new(),
            _phantom: PhantomData,
        }
    }
}

fn push_and_get_index<V>(vec: &mut Vec<V>, value: V) -> u32 {
    let index = vec.len();
    vec.push(value);
    index.try_into().unwrap()
}

impl<
        R: Frame,
        S: GraphicsPipelineListBuilder,
        M: MaterialPackListBuilder,
        V: MeshPackListBuilder,
    > VulkanContextBuilder<R, S, M, V>
{
    pub fn with_material_type<N: Material>(self) -> VulkanContextBuilder<R, S, Cons<Vec<N>, M>, V> {
        VulkanContextBuilder {
            materials: Cons {
                head: vec![],
                tail: self.materials,
            },
            meshes: self.meshes,
            shaders: self.shaders,
            _phantom: PhantomData,
        }
    }

    pub fn with_mesh_type<N: Vertex>(self) -> VulkanContextBuilder<R, S, M, Cons<Vec<Mesh<N>>, V>> {
        VulkanContextBuilder {
            meshes: Cons {
                head: vec![],
                tail: self.meshes,
            },
            materials: self.materials,
            shaders: self.shaders,
            _phantom: PhantomData,
        }
    }

    pub fn with_shader_type<N: ShaderType + Into<R::Shader<N>>>(
        self,
    ) -> VulkanContextBuilder<R, Cons<Vec<R::Shader<N>>, S>, M, V> {
        VulkanContextBuilder {
            shaders: Cons {
                head: vec![],
                tail: self.shaders,
            },
            materials: self.materials,
            meshes: self.meshes,
            _phantom: PhantomData,
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

    pub fn add_shader<N: ShaderType + Into<R::Shader<N>>, T: Marker>(
        &mut self,
        shader: N,
    ) -> ShaderHandle<N>
    where
        S: Contains<Vec<R::Shader<N>>, T>,
    {
        ShaderHandle::new(push_and_get_index(self.shaders.get_mut(), shader.into()))
    }
}

impl<
        R: Frame,
        M: MaterialPackList + 'static,
        V: MeshPackList + 'static,
        S: GraphicsPipelinePackList + 'static,
    > RendererContext for VulkanRendererContext<R, M, V, S>
{
    type Renderer = VulkanRenderer;
    type Shaders = S;
    type Materials = M;
    type Meshes = V;

    fn begin_frame<C: Camera>(&mut self, camera: &C) -> Result<(), Box<dyn Error>> {
        let context = self.context.borrow();
        let camera_matrices = camera.get_matrices();
        self.resources
            .renderer_context
            .begin_frame(&context, &camera_matrices)?;
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Box<dyn Error>> {
        let context = self.context.borrow();
        self.resources.renderer_context.end_frame(&context)?;
        Ok(())
    }

    fn draw<T: ShaderType, D: Drawable<Material = T::Material, Vertex = T::Vertex>>(
        &mut self,
        shader: ShaderHandle<T>,
        drawable: &D,
        transform: &Matrix4,
    ) -> Result<(), Box<dyn Error>> {
        self.resources.renderer_context.draw(
            &self.context.borrow(),
            shader,
            drawable,
            transform,
            &self.resources.materials,
            &self.resources.meshes,
        );
        Ok(())
    }
}
