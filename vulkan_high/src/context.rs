use graphics::shader::Shader;
use math::types::Matrix4;
use type_kit::{Cons, Contains, Create, Destroy, DestroyResult, Marker, Nil};
use vulkan_low::Context;

use graphics::renderer::{camera::Camera, ContextBuilder, RendererContext};
use graphics::{
    model::{Drawable, Material, MaterialHandle, Mesh, MeshHandle, Vertex},
    shader::{ShaderHandle, ShaderType},
};
use std::convert::Infallible;
use std::ops::Deref;
use std::path::Path;
use std::rc::Rc;
use std::{error::Error, marker::PhantomData};
use winit::window::Window;

use vulkan_low::memory::allocator::{AllocatorBuilder, AllocatorIndexTyped, Static, StaticConfig};
use vulkan_low::resources::Partial;

use crate::renderer::deferred::{DeferredRendererBuilder, DeferredShader};
use crate::renderer::storage::DrawStorageTyped;
use crate::renderer::{Renderer, RendererBuilder};
use crate::resources::{
    CommonResources, CommonResourcesPartial, GraphicsPipelineListBuilder, GraphicsPipelinePackList,
    MaterialPackList, MaterialPackListBuilder, MaterialPackListPartial, MeshPackList,
    MeshPackListBuilder, MeshPackListPartial,
};
use crate::{VulkanRenderer, VulkanRendererConfig};

pub struct VulkanContext {
    context: Context,
    common_resources: CommonResources,
    allocator: AllocatorIndexTyped<Static>,
    _config: VulkanRendererConfig,
}

impl VulkanContext {
    #[inline]
    pub fn common_resources(&self) -> &CommonResources {
        &self.common_resources
    }
}

impl Deref for VulkanContext {
    type Target = Context;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        let _ = self.context.wait_idle();
        let _ = self.common_resources.destroy(&self.context);
        let _ = self.context.destroy_allocator(self.allocator);
    }
}

pub struct VulkanResourcePack<M: MaterialPackList, V: MeshPackList, P: GraphicsPipelinePackList> {
    pub materials: M,
    pub meshes: V,
    pub pipelines: P,
}

struct VulkanResourcePackPartial<
    'a,
    M: MaterialPackList,
    V: MeshPackList,
    P: GraphicsPipelinePackList,
    PM: MaterialPackListPartial<Pack = M>,
    PV: MeshPackListPartial<Pack = V>,
> where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    materials: PM,
    meshes: PV,
    pipelines: P,
    _phantom: PhantomData<&'a ()>,
}

impl<
        'a,
        M: MaterialPackList,
        V: MeshPackList,
        P: GraphicsPipelinePackList,
        PM: MaterialPackListPartial<Pack = M>,
        PV: MeshPackListPartial<Pack = V>,
    > VulkanResourcePackPartial<'a, M, V, P, PM, PV>
where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    fn load(
        self,
        context: &Context,
        allocator: AllocatorIndexTyped<Static>,
    ) -> Result<VulkanResourcePack<M, V, P>, Box<dyn Error>> {
        let Self {
            materials,
            meshes,
            pipelines,
            ..
        } = self;
        let meshes = meshes.allocate(context, allocator.into())?;
        let materials = materials.allocate(context, allocator.into())?;
        Ok(VulkanResourcePack {
            materials,
            meshes,
            pipelines,
        })
    }
}

impl<
        'a,
        M: MaterialPackList,
        V: MeshPackList,
        P: GraphicsPipelinePackList,
        PM: MaterialPackListPartial<Pack = M>,
        PV: MeshPackListPartial<Pack = V>,
    > Destroy for VulkanResourcePackPartial<'a, M, V, P, PM, PV>
where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    type Context<'b> = &'b Context;
    type DestroyError = Infallible;
    fn destroy<'b>(&mut self, context: Self::Context<'b>) -> DestroyResult<Self> {
        let _ = self.materials.destroy(context);
        let _ = self.meshes.destroy(context);
        let _ = self.pipelines.destroy(context);
        Ok(())
    }
}
impl<
        'a,
        M: MaterialPackList,
        V: MeshPackList,
        P: GraphicsPipelinePackList,
        PM: MaterialPackListPartial<Pack = M>,
        PV: MeshPackListPartial<Pack = V>,
    > Partial for VulkanResourcePackPartial<'a, M, V, P, PM, PV>
where
    for<'b> PM: Destroy<Context<'b> = &'b Context>,
    for<'b> PV: Destroy<Context<'b> = &'b Context>,
{
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.materials.register_memory_requirements(builder);
        self.meshes.register_memory_requirements(builder);
    }
}

type PackPartial<'a, M, V, P, MB, MV> = VulkanResourcePackPartial<
    'a,
    M,
    V,
    P,
    <MB as MaterialPackListBuilder>::Partial<'a>,
    <MV as MeshPackListBuilder>::Partial<'a>,
>;

impl<M: MaterialPackList, V: MeshPackList, P: GraphicsPipelinePackList>
    VulkanResourcePack<M, V, P>
{
    fn prepare<
        'a,
        MB: MaterialPackListBuilder<Pack = M>,
        MV: MeshPackListBuilder<Pack = V>,
        MP: GraphicsPipelineListBuilder<Pack = P>,
    >(
        context: &Context,
        materials: &'a MB,
        meshes: &'a MV,
        pipelines: &'a MP,
    ) -> Result<PackPartial<'a, M, V, P, MB, MV>, Box<dyn Error>> {
        let materials = materials.prepare(context)?;
        let meshes = meshes.prepare(context)?;
        let pipelines = pipelines.build(context)?;
        Ok(VulkanResourcePackPartial {
            materials,
            meshes,
            pipelines,
            _phantom: PhantomData,
        })
    }
}

impl<M: MaterialPackList, V: MeshPackList, P: GraphicsPipelinePackList> Destroy
    for VulkanResourcePack<M, V, P>
{
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.materials.destroy(context);
        let _ = self.meshes.destroy(context);
        let _ = self.pipelines.destroy(context);
        Ok(())
    }
}

pub struct VulkanRendererContext<M: MaterialPackList, V: MeshPackList, P: GraphicsPipelinePackList>
{
    // TODO: Remove dynamic dispatch here
    renderer: Box<dyn Renderer>,
    draw_storage: DrawStorageTyped<M, V, P>,
    context: Rc<VulkanContext>,
    allocator: AllocatorIndexTyped<Static>,
}

impl VulkanContext {
    pub fn new(window: &Window, config: VulkanRendererConfig) -> Result<Self, Box<dyn Error>> {
        let context = Context::build(window)?;
        let common_resources = CommonResourcesPartial::create((), &context)?;
        let mut allocator_config = StaticConfig::new();
        common_resources.register_memory_requirements(&mut allocator_config);
        let allocator = context.create_allocator::<Static, _>(allocator_config)?;
        let common_resources =
            CommonResources::create((common_resources, allocator.into()), &context)?;
        Ok(VulkanContext {
            context,
            common_resources,
            allocator,
            _config: config,
        })
    }
}

impl<'a, M: MaterialPackList, V: MeshPackList, S: GraphicsPipelinePackList> Drop
    for VulkanRendererContext<M, V, S>
{
    fn drop(&mut self) {
        let _ = self.context.wait_idle();
        let _ = self.draw_storage.destroy(&self.context);
        let _ = self.renderer.destroy(&self.context);
        let _ = self.context.destroy_allocator(self.allocator);
    }
}

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
    type Context<'a> = VulkanRendererContext<M::Pack, V::Pack, P::Pack>;

    fn build<'a>(self, context: &'a Self::Renderer) -> Result<Self::Context<'a>, Box<dyn Error>> {
        let context = context.context.clone();
        let mut config = StaticConfig::new();
        let resources = VulkanResourcePack::prepare(
            &context.context,
            &self.materials,
            &self.meshes,
            &self.shaders,
        )?;
        let renderer = DeferredRendererBuilder::<P::Pack>::new(
            context.clone(),
            Path::new("_resources/assets/skybox/skybox"),
        )?;
        resources.register_memory_requirements(&mut config);
        renderer.register_memory_requirements(&mut config);
        let allocator = context.create_allocator::<Static, _>(config)?;
        let resources = resources.load(&context, allocator)?;
        let draw_storage = DrawStorageTyped::new(resources);
        let renderer = Box::new(renderer.build()?);
        Ok(VulkanRendererContext {
            context,
            renderer,
            draw_storage,
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

type ShaderCollection<V, M> = Vec<DeferredShader<Shader<V, M>>>;

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
    > RendererContext for VulkanRendererContext<M, V, S>
{
    type Shaders = S;
    type Materials = M;
    type Meshes = V;

    fn begin_frame<C: Camera>(&mut self, camera: &C) -> Result<(), Box<dyn Error>> {
        let camera_matrices = camera.get_matrices();
        let camera_descriptor = self.renderer.begin_frame(&self.context, camera_matrices)?;
        self.draw_storage.begin_frame(camera_descriptor);
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Box<dyn Error>> {
        let draw_storage = self.draw_storage.end_frame();
        self.renderer.render(&self.context, draw_storage)?;
        Ok(())
    }

    fn draw<T: ShaderType, D: Drawable<Material = T::Material, Vertex = T::Vertex>>(
        &mut self,
        shader: ShaderHandle<T>,
        drawable: &D,
        transform: &Matrix4,
    ) -> Result<(), Box<dyn Error>> {
        // Currently only Deferred Shaders are supported
        // TODO: Implement mapper from user facing common Shader type
        // to ShaderType + GraphicsPipelineConfig type used by configured rendering pipeline
        let shader = shader.map::<DeferredShader<T>>();
        self.draw_storage
            .append_draw_call(&self.context, shader, drawable, transform);
        Ok(())
    }
}
