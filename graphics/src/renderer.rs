pub mod camera;

use math::types::Matrix4;
use std::marker::PhantomData;
use type_kit::{Cons, Contains, Marker, Nil};
use winit::window::Window;

use crate::{
    error::{GraphicsError, GraphicsResult},
    model::{Material, MaterialHandleTyped, Mesh, MeshHandleTyped, Model, ModelTyped, Vertex},
    shader::{Shader, ShaderHandle, ShaderHandleTyped, ShaderType},
};

use self::camera::Camera;

pub trait Renderer: 'static {
    fn context_builder() -> impl ContextBuilder<Renderer = Self>;
}

pub trait DrawMapper {
    fn try_draw(
        renderer: &mut impl RendererContext,
        shader: ShaderHandle,
        model: Model,
        transform: &Matrix4,
    ) -> GraphicsResult<()>;
}

impl DrawMapper for Nil {
    fn try_draw(
        _: &mut impl RendererContext,
        shader: ShaderHandle,
        model: Model,
        _: &Matrix4,
    ) -> GraphicsResult<()> {
        Err(GraphicsError::InvalidDrawCall { shader, model })
    }
}

impl<V: Vertex, M: Material, N: DrawMapper> DrawMapper for Cons<Vec<Shader<V, M>>, N> {
    fn try_draw(
        renderer: &mut impl RendererContext,
        shader: ShaderHandle,
        model: Model,
        transform: &Matrix4,
    ) -> GraphicsResult<()> {
        let shader_typed: Result<ShaderHandleTyped<Shader<V, M>>, _> = shader.try_into();
        let model_typed: Result<ModelTyped<M, V>, _> = model.try_into();
        if let (Ok(shader_typed), Ok(model_typed)) = (shader_typed, model_typed) {
            renderer.draw_typed(shader_typed, model_typed, transform)?;
            Ok(())
        } else {
            N::try_draw(renderer, shader, model, transform)
        }
    }
}

pub struct Context<R: RendererContext, M: DrawMapper> {
    context: R,
    _phantom: PhantomData<M>,
}

impl<R: RendererContext, M: DrawMapper> Context<R, M> {
    #[inline]
    pub fn begin_frame<C: Camera>(&mut self, camera: &C) -> GraphicsResult<()> {
        self.context.begin_frame(camera)
    }

    #[inline]
    pub fn end_frame(&mut self) -> GraphicsResult<()> {
        self.context.end_frame()
    }

    #[inline]
    pub fn draw_typed<S: ShaderType>(
        &mut self,
        shader: ShaderHandleTyped<S>,
        model: ModelTyped<S::Material, S::Vertex>,
        transform: &Matrix4,
    ) -> GraphicsResult<()> {
        self.context.draw_typed(shader, model, transform)
    }

    #[inline]
    pub fn draw(
        &mut self,
        shader: ShaderHandle,
        model: Model,
        transform: &Matrix4,
    ) -> GraphicsResult<()> {
        M::try_draw(&mut self.context, shader, model, transform)
    }
}

pub fn create_context<B: ContextBuilder>(
    renderer: &mut B::Renderer,
    builder: B,
) -> GraphicsResult<Context<impl RendererContext + use<'_, B>, B::Shaders>> {
    Ok(Context {
        context: builder.build(renderer)?,
        _phantom: PhantomData,
    })
}

pub trait ContextBuilder {
    type Shaders: DrawMapper;
    type Materials;
    type Meshes;

    type Renderer: Renderer;

    fn build(self, renderer: &mut Self::Renderer) -> GraphicsResult<impl RendererContext>;

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

    fn add_material<N: Material, T: Marker>(&mut self, material: N) -> MaterialHandleTyped<N>
    where
        Self::Materials: Contains<Vec<N>, T>;

    fn add_mesh<N: Vertex, T: Marker>(&mut self, mesh: Mesh<N>) -> MeshHandleTyped<N>
    where
        Self::Meshes: Contains<Vec<Mesh<N>>, T>;

    fn add_shader<N: Vertex, T: Material, K: Marker>(
        &mut self,
        shader: Shader<N, T>,
    ) -> ShaderHandleTyped<Shader<N, T>>
    where
        Self::Shaders: Contains<Vec<Shader<N, T>>, K>;
}

pub trait RendererContext {
    type Shaders;
    type Materials;
    type Meshes;

    fn begin_frame<C: Camera>(&mut self, camera: &C) -> GraphicsResult<()>;
    fn end_frame(&mut self) -> GraphicsResult<()>;
    fn draw_typed<S: ShaderType>(
        &mut self,
        shader: ShaderHandleTyped<S>,
        model: ModelTyped<S::Material, S::Vertex>,
        transform: &Matrix4,
    ) -> GraphicsResult<()>;
}

pub trait RendererBuilder {
    fn build(self, window: &Window) -> GraphicsResult<impl Renderer + use<Self>>;
}
