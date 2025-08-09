pub mod camera;

use math::types::Matrix4;
use std::{error::Error, rc::Rc};
use type_kit::{Cons, Contains, Marker};
use winit::window::Window;

use crate::{
    model::{Drawable, Material, MaterialHandle, Mesh, MeshHandle, Vertex},
    shader::{Shader, ShaderHandle, ShaderType},
};

use self::camera::Camera;

pub trait Renderer {
    fn context_builder() -> impl ContextBuilder<Renderer = Self>;
}

pub trait ContextBuilder {
    type Materials;
    type Shaders;
    type Meshes;

    type Renderer: Renderer;

    fn build(self, renderer: &mut Self::Renderer) -> Result<impl RendererContext, Box<dyn Error>>;

    fn with_material_type<N: Material>(
        self,
    ) -> impl ContextBuilder<
        Renderer = Self::Renderer,
        Materials = Cons<Vec<N>, Self::Materials>,
        Shaders = Self::Shaders,
        Meshes = Self::Meshes,
    >;

    fn with_mesh_type<N: Vertex>(
        self,
    ) -> impl ContextBuilder<
        Renderer = Self::Renderer,
        Materials = Self::Materials,
        Shaders = Self::Shaders,
        Meshes = Cons<Vec<Mesh<N>>, Self::Meshes>,
    >;

    fn with_shader_type<N: Vertex, T: Material>(
        self,
    ) -> impl ContextBuilder<
        Renderer = Self::Renderer,
        Materials = Self::Materials,
        Shaders = Cons<Vec<Shader<N, T>>, Self::Shaders>,
        Meshes = Self::Meshes,
    >;

    fn add_material<N: Material, T: Marker>(&mut self, material: N) -> MaterialHandle<N>
    where
        Self::Materials: Contains<Vec<N>, T>;

    fn add_mesh<N: Vertex, T: Marker>(&mut self, mesh: Mesh<N>) -> MeshHandle<N>
    where
        Self::Meshes: Contains<Vec<Mesh<N>>, T>;

    fn add_shader<N: Vertex, T: Material, K: Marker>(
        &mut self,
        shader: Shader<N, T>,
    ) -> ShaderHandle<Shader<N, T>>
    where
        Self::Shaders: Contains<Vec<Shader<N, T>>, K>;
}

pub trait RendererContext {
    type Shaders;
    type Materials;
    type Meshes;

    fn begin_frame<C: Camera>(&mut self, camera: &C) -> Result<(), Box<dyn Error>>;
    fn end_frame(&mut self) -> Result<(), Box<dyn Error>>;
    fn draw<S: ShaderType, D: Drawable<Material = S::Material, Vertex = S::Vertex>>(
        &mut self,
        shader: ShaderHandle<S>,
        drawable: &D,
        transform: &Matrix4,
    ) -> Result<(), Box<dyn Error>>;
}

pub trait RendererBuilder {
    fn build(self, window: Rc<Window>) -> Result<impl Renderer, Box<dyn Error>>;
}
