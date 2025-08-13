pub mod ecs;

use type_kit::{Cons, Nil};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::KeyCode,
    window::{Window, WindowBuilder},
};

use math::{transform::Transform, types::Matrix4};
use std::{cell::RefCell, error::Error, rc::Rc, time::Instant};

use graphics::{
    model::Drawable,
    shader::{ShaderHandle, ShaderType},
};

use graphics::renderer::{
    camera::{Camera, CameraBuilder, CameraNone},
    ContextBuilder, Renderer, RendererBuilder, RendererContext,
};
use input::InputHandler;

#[derive(Clone, Copy)]
pub struct DrawCommand<S: ShaderType, D: Drawable<Material = S::Material, Vertex = S::Vertex>> {
    shader: ShaderHandle<S>,
    model: D,
    transform: Matrix4,
}

pub struct Object<D: Drawable + Clone + Copy> {
    model: D,
    transform: Transform,
    update: Box<dyn Fn(f32, Transform) -> Transform>,
}

impl<D: Drawable + Clone + Copy> Object<D> {
    pub fn new(
        model: D,
        transform: Transform,
        update: Box<dyn Fn(f32, Transform) -> Transform>,
    ) -> Self {
        Self {
            model,
            transform,
            update,
        }
    }

    fn update<S: ShaderType<Vertex = D::Vertex, Material = D::Material>>(
        &mut self,
        shader: ShaderHandle<S>,
        elapsed_time: f32,
    ) -> DrawCommand<S, D> {
        self.transform = (self.update)(elapsed_time, self.transform);
        DrawCommand {
            shader,
            model: self.model,
            transform: self.transform.into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum CursorState {
    Locked,
    Free,
}

impl CursorState {
    pub fn new() -> Self {
        Self::Free
    }
    pub fn switch(&mut self, window: &Window) -> Result<(), Box<dyn Error>> {
        *self = match self {
            Self::Free => {
                let window_extent = window.inner_size();
                window.set_cursor_grab(winit::window::CursorGrabMode::Confined)?;
                window.set_cursor_position(PhysicalPosition {
                    x: window_extent.width / 2,
                    y: window_extent.height / 2,
                })?;
                window.set_cursor_visible(false);
                Self::Locked
            }
            Self::Locked => {
                window.set_cursor_grab(winit::window::CursorGrabMode::None)?;
                window.set_cursor_visible(true);
                Self::Free
            }
        };
        Ok(())
    }
}

pub struct LoopBuilder<R: RendererBuilder, C: CameraBuilder> {
    renderer: R,
    camera: Option<C>,
    window: Option<WindowBuilder>,
}

impl<N: RendererBuilder> LoopBuilder<N, CameraNone> {
    pub fn new(renderer: N) -> Self {
        Self {
            camera: None,
            window: None,
            renderer,
        }
    }
}

impl<R: RendererBuilder, C: CameraBuilder> LoopBuilder<R, C> {
    pub fn with_window(self, window: WindowBuilder) -> Self {
        Self {
            window: Some(window),
            ..self
        }
    }

    pub fn with_camera<N: CameraBuilder>(self, camera: N) -> LoopBuilder<R, N> {
        let Self {
            window, renderer, ..
        } = self;
        LoopBuilder {
            camera: Some(camera),
            window,
            renderer,
        }
    }

    pub fn build(self) -> Result<Loop<impl Renderer, C::Camera>, Box<dyn Error>> {
        let Self {
            window,
            renderer,
            camera,
        } = self;
        let mut input_handler = InputHandler::new();
        let event_loop = EventLoop::new()?;
        let window = Rc::new(
            window
                .ok_or("Window configuration not provided for Loop!")?
                .build(&event_loop)?,
        );
        let renderer = renderer.build(window.clone())?;
        let camera = camera
            .ok_or("Camera not selected for Loop!")?
            .build(&mut input_handler);
        Ok(Loop {
            event_loop,
            window,
            renderer,
            input_handler,
            camera,
        })
    }
}

pub trait DrawableTypeList: 'static {
    const LEN: usize;
    type Drawable: Drawable + Clone + Copy;
    type Next: DrawableTypeList;
}

impl DrawableTypeList for Nil {
    const LEN: usize = 0;
    type Drawable = Self;
    type Next = Self;
}

pub struct DrawableContainer<
    S: ShaderType,
    D: Drawable<Material = S::Material, Vertex = S::Vertex> + Clone + Copy,
> {
    shader: ShaderHandle<S>,
    objects: Vec<Object<D>>,
}

impl<
        S: ShaderType,
        D: Drawable<Material = S::Material, Vertex = S::Vertex> + Clone + Copy,
        N: DrawableTypeList,
    > DrawableTypeList for Cons<DrawableContainer<S, D>, N>
{
    const LEN: usize = N::LEN + 1;
    type Drawable = D;
    type Next = N;
}

pub trait DrawCommandCollection: DrawableTypeList {
    fn draw<R: RendererContext>(self, renderer: &mut R);
}

impl DrawCommandCollection for Nil {
    fn draw<R: RendererContext>(self, _renderer: &mut R) {}
}

impl<
        S: ShaderType,
        D: Drawable<Vertex = S::Vertex, Material = S::Material> + Clone + Copy,
        N: DrawCommandCollection,
    > DrawableTypeList for Cons<Vec<DrawCommand<S, D>>, N>
{
    const LEN: usize = N::LEN + 1;
    type Drawable = D;
    type Next = N;
}

impl<
        S: ShaderType,
        D: Drawable<Vertex = S::Vertex, Material = S::Material> + Clone + Copy,
        N: DrawCommandCollection,
    > DrawCommandCollection for Cons<Vec<DrawCommand<S, D>>, N>
{
    fn draw<R: RendererContext>(self, renderer: &mut R) {
        for DrawCommand {
            shader,
            model,
            transform,
        } in self.head
        {
            let _ = renderer.draw(shader, &model, &transform);
        }
        self.tail.draw(renderer);
    }
}

pub trait DrawableCollection: DrawableTypeList {
    type DrawCommands: DrawCommandCollection;
    fn update(&mut self, elapsed_time: f32) -> Self::DrawCommands;
}

impl DrawableCollection for Nil {
    type DrawCommands = Self;
    fn update(&mut self, _elapsed_time: f32) -> Self::DrawCommands {
        Nil::new()
    }
}

impl<
        S: ShaderType,
        D: Drawable<Vertex = S::Vertex, Material = S::Material> + Clone + Copy,
        N: DrawableCollection,
    > DrawableCollection for Cons<DrawableContainer<S, D>, N>
{
    type DrawCommands = Cons<Vec<DrawCommand<S, D>>, N::DrawCommands>;

    fn update(&mut self, elapsed_time: f32) -> Self::DrawCommands {
        let draw = self
            .head
            .objects
            .iter_mut()
            .map(|object| object.update(self.head.shader, elapsed_time))
            .collect();
        Cons {
            head: draw,
            tail: self.tail.update(elapsed_time),
        }
    }
}

pub struct Loop<R: Renderer, C: Camera> {
    renderer: R,
    window: Rc<Window>,
    event_loop: EventLoop<()>,
    input_handler: InputHandler,
    camera: Rc<RefCell<C>>,
}

pub trait LoopTypes {
    type Renderer: Renderer;
    type Camera: Camera;
}

impl<R: Renderer, C: Camera> LoopTypes for Loop<R, C> {
    type Renderer = R;
    type Camera = C;
}

pub struct Scene<D: DrawableCollection, B: ContextBuilder> {
    renderer_context: B,
    objects: D,
}

impl<D: DrawableCollection, B: ContextBuilder> Scene<D, B> {
    pub fn with_objects<
        S: ShaderType,
        T: Drawable<Vertex = S::Vertex, Material = S::Material> + Clone + Copy,
    >(
        self,
        shader: ShaderHandle<S>,
        objects: Vec<Object<T>>,
    ) -> Scene<Cons<DrawableContainer<S, T>, D>, B> {
        Scene {
            renderer_context: self.renderer_context,
            objects: Cons {
                head: DrawableContainer { shader, objects },
                tail: self.objects,
            },
        }
    }
}

impl<R: Renderer, C: Camera> Loop<R, C> {
    pub fn renderer_context_builder(&self) -> impl ContextBuilder<Renderer = R> {
        R::context_builder()
    }

    pub fn scene<B: ContextBuilder<Renderer = R>>(
        &self,
        builder: B,
    ) -> Result<Scene<Nil, B>, Box<dyn Error>> {
        Ok(Scene {
            renderer_context: builder,
            objects: Nil::new(),
        })
    }

    pub fn run<D: DrawableCollection, B: ContextBuilder<Renderer = R>>(
        self,
        mut scene: Scene<D, B>,
    ) -> Result<(), Box<dyn Error>> {
        let Self {
            window,
            event_loop,
            mut renderer,
            mut input_handler,
            camera,
        } = self;
        let mut context = scene.renderer_context.build(&mut renderer)?;
        let cursor_state = Rc::new(RefCell::new(CursorState::new()));
        let shared_cursor_state = cursor_state.clone();
        let shared_window = window.clone();
        let shared_camera = camera.clone();
        input_handler.register_key_state_callback(
            KeyCode::KeyG,
            Box::new(move |state| {
                if let ElementState::Pressed = state {
                    let _ = shared_cursor_state.borrow_mut().switch(&shared_window);
                    match *(*shared_cursor_state).borrow() {
                        CursorState::Free => shared_camera.borrow_mut().set_active(false),
                        CursorState::Locked => shared_camera.borrow_mut().set_active(true),
                    }
                }
            }),
        );
        let mut draw_commands = None;
        let mut previous_frame_time = Instant::now();
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run(|event, elwt| {
            input_handler.handle_event(event.clone());
            match event {
                Event::NewEvents(StartCause::Poll) => {
                    let current_frame_time = Instant::now();
                    let elapsed_time = (current_frame_time - previous_frame_time).as_secs_f32();
                    previous_frame_time = current_frame_time;

                    camera.borrow_mut().update(elapsed_time);
                    draw_commands = Some(scene.objects.update(elapsed_time));
                    if let CursorState::Locked = *(*cursor_state).borrow() {
                        let window_extent = window.inner_size();
                        let _ = window.set_cursor_position(PhysicalPosition {
                            x: window_extent.width / 2,
                            y: window_extent.height / 2,
                        });
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    elwt.exit();
                }
                Event::AboutToWait => {
                    let camera: &C = &(*camera).borrow();
                    let _ = context.begin_frame(camera);
                    if let Some(draw_commands) = draw_commands.take() {
                        draw_commands.draw(&mut context);
                    }
                    let _ = context.end_frame();
                }
                _ => (),
            }
        })?;
        Ok(())
    }
}
