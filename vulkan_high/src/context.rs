use graphics::shader::Shader;
use math::types::Matrix4;
use type_kit::{Cons, Contains, Destroy, Marker, Nil};

use graphics::renderer::{camera::Camera, ContextBuilder};
use graphics::{
    model::{Drawable, Material, MaterialHandle, Mesh, MeshHandle, Vertex},
    shader::{ShaderHandle, ShaderType},
};
use std::error::Error;
use std::marker::PhantomData;
use std::rc::Rc;

use vulkan_low::memory::allocator::{AllocatorIndexTyped, Static, StaticConfig};
use vulkan_low::resources::Partial;

use crate::renderer::storage::DrawCallRecorder;
use crate::renderer::{Renderer, RendererContext};
use crate::resources::{
    GraphicsPipelineListBuilder, GraphicsPipelinePackList, MaterialPackList,
    MaterialPackListBuilder, MeshPackList, MeshPackListBuilder, ResourcePack,
};
use crate::{VulkanContext, VulkanRenderer};

pub struct VulkanRendererContext<
    'a,
    M: MaterialPackList,
    V: MeshPackList,
    P: GraphicsPipelinePackList,
    R: Renderer,
> {
    context: Rc<VulkanContext>,
    renderer: R::RendererContext<'a, P>,
    resources: ResourcePack<R, M, V, P>,
    draw_recorder: DrawCallRecorder<R, M, V, P>,
    allocator: AllocatorIndexTyped<Static>,
}

impl<'a, M: MaterialPackList, V: MeshPackList, S: GraphicsPipelinePackList, R: Renderer> Drop
    for VulkanRendererContext<'a, M, V, S, R>
{
    fn drop(&mut self) {
        let _ = self.context.wait_idle();
        let _ = self.renderer.destroy(&self.context);
        let _ = self.resources.destroy(&self.context);
        let _ = self.context.destroy_allocator(self.allocator);
    }
}

#[derive(Debug)]
pub struct VulkanContextBuilder<
    R: Renderer,
    P: GraphicsPipelineListBuilder,
    M: MaterialPackListBuilder,
    V: MeshPackListBuilder,
> {
    shaders: P,
    materials: M,
    meshes: V,
    _renderer: PhantomData<R>,
}

impl<
        R: Renderer,
        P: GraphicsPipelineListBuilder,
        M: MaterialPackListBuilder,
        V: MeshPackListBuilder,
    > ContextBuilder for VulkanContextBuilder<R, P, M, V>
{
    type Materials = M;
    type Shaders = P;
    type Meshes = V;
    type Renderer = VulkanRenderer<R>;

    fn build(
        self,
        renderer: &mut Self::Renderer,
    ) -> Result<impl graphics::renderer::RendererContext, Box<dyn Error>> {
        let context = renderer.shared_context();
        let mut config = StaticConfig::new();
        let resources = ResourcePack::<R, _, _, _>::prepare(
            &context,
            &self.materials,
            &self.meshes,
            self.shaders,
        )?;
        resources.register_memory_requirements(&mut config);
        let allocator = context.create_allocator(config)?;
        let resources = resources.with_allocator(allocator).build(&context)?;
        let renderer = renderer.load_context::<P::Pack<R>>(&context)?;
        let draw_storage = DrawCallRecorder::new();
        Ok(VulkanRendererContext {
            context,
            renderer,
            resources,
            draw_recorder: draw_storage,
            allocator,
        })
    }

    fn with_material_type<N: Material>(
        self,
    ) -> impl ContextBuilder<
        Renderer = Self::Renderer,
        Materials = Cons<Vec<N>, Self::Materials>,
        Shaders = Self::Shaders,
        Meshes = Self::Meshes,
    > {
        VulkanContextBuilder {
            materials: Cons::<Vec<N>, _> {
                head: vec![],
                tail: self.materials,
            },
            meshes: self.meshes,
            shaders: self.shaders,
            _renderer: PhantomData,
        }
    }

    fn with_mesh_type<N: Vertex>(
        self,
    ) -> impl ContextBuilder<
        Renderer = Self::Renderer,
        Materials = Self::Materials,
        Shaders = Self::Shaders,
        Meshes = Cons<Vec<Mesh<N>>, Self::Meshes>,
    > {
        VulkanContextBuilder {
            meshes: Cons::<Vec<Mesh<N>>, _> {
                head: vec![],
                tail: self.meshes,
            },
            materials: self.materials,
            shaders: self.shaders,
            _renderer: PhantomData,
        }
    }

    fn with_shader_type<N: Vertex, T: Material>(
        self,
    ) -> impl ContextBuilder<
        Renderer = Self::Renderer,
        Materials = Self::Materials,
        Shaders = Cons<Vec<Shader<N, T>>, Self::Shaders>,
        Meshes = Self::Meshes,
    > {
        VulkanContextBuilder {
            shaders: Cons::<Vec<Shader<N, T>>, _> {
                head: vec![],
                tail: self.shaders,
            },
            materials: self.materials,
            meshes: self.meshes,
            _renderer: PhantomData,
        }
    }

    fn add_material<N: Material, T: Marker>(&mut self, material: N) -> MaterialHandle<N>
    where
        Self::Materials: Contains<Vec<N>, T>,
    {
        MaterialHandle::new(push_and_get_index(self.materials.get_mut(), material))
    }

    fn add_mesh<N: Vertex, T: Marker>(&mut self, mesh: Mesh<N>) -> MeshHandle<N>
    where
        Self::Meshes: Contains<Vec<Mesh<N>>, T>,
    {
        MeshHandle::new(push_and_get_index(self.meshes.get_mut(), mesh))
    }

    fn add_shader<N: Vertex, T: Material, K: Marker>(
        &mut self,
        shader: Shader<N, T>,
    ) -> ShaderHandle<Shader<N, T>>
    where
        Self::Shaders: Contains<Vec<Shader<N, T>>, K>,
    {
        ShaderHandle::new(push_and_get_index(self.shaders.get_mut(), shader.into()))
    }
}

impl<R: Renderer> Default for VulkanContextBuilder<R, Nil, Nil, Nil> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Renderer> VulkanContextBuilder<R, Nil, Nil, Nil> {
    pub fn new() -> Self {
        VulkanContextBuilder {
            shaders: Nil::new(),
            materials: Nil::new(),
            meshes: Nil::new(),
            _renderer: PhantomData,
        }
    }
}

fn push_and_get_index<V>(vec: &mut Vec<V>, value: V) -> u32 {
    let index = vec.len();
    vec.push(value);
    index.try_into().unwrap()
}

impl<
        'a,
        M: MaterialPackList + 'static,
        V: MeshPackList + 'static,
        S: GraphicsPipelinePackList + 'static,
        R: Renderer,
    > graphics::renderer::RendererContext for VulkanRendererContext<'a, M, V, S, R>
{
    type Shaders = S;
    type Materials = M;
    type Meshes = V;

    fn begin_frame<C: Camera>(&mut self, camera: &C) -> Result<(), Box<dyn Error>> {
        let camera_matrices = camera.get_matrices();
        let camera_descriptor = self.renderer.begin_frame(&self.context, camera_matrices)?;
        self.draw_recorder.begin_frame(camera_descriptor);
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Box<dyn Error>> {
        let draw_storage = self.draw_recorder.end_frame();
        self.renderer.render(&self.context, draw_storage)?;
        Ok(())
    }

    fn draw<T: ShaderType, D: Drawable<Material = T::Material, Vertex = T::Vertex>>(
        &mut self,
        shader: ShaderHandle<T>,
        drawable: &D,
        transform: &Matrix4,
    ) -> Result<(), Box<dyn Error>> {
        let shader = shader.map::<R::ShaderType<T>>();
        self.draw_recorder.append_draw_call(
            &self.context,
            &self.resources,
            shader,
            drawable,
            transform,
        );
        Ok(())
    }
}
