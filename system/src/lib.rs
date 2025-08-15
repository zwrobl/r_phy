use input::InputHandler;
use type_kit::{Cons, Nil};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::KeyCode,
    window::{Window, WindowBuilder},
};

use math::{transform::Transform, types::Matrix4};
use std::{cell::RefCell, error::Error, rc::Rc, time::Instant, vec};

use graphics::{
    model::{Material, Model, ModelTyped, Vertex}, renderer::create_context, shader::{ShaderHandle, ShaderHandleTyped, ShaderType}
};

use graphics::renderer::{
    camera::{Camera, CameraBuilder, CameraNone},
    ContextBuilder, Renderer, RendererBuilder,
};

#[derive(Clone, Copy)]
pub struct DrawCommand {
    shader: ShaderHandle,
    model: Model,
    transform: Matrix4,
}

pub struct Object<V: Vertex, M: Material> {
    model: ModelTyped<M, V>,
    transform: Transform,
    update: Box<dyn Fn(f32, Transform) -> Transform>,
}

impl<V: Vertex, M: Material> Object<V, M> {
    pub fn new(
        model: ModelTyped<M, V>,
        transform: Transform,
        update: Box<dyn Fn(f32, Transform) -> Transform>,
    ) -> Self {
        Self {
            model,
            transform,
            update,
        }
    }

    fn update<S: ShaderType<Vertex = V, Material = M>>(
        &mut self,
        shader: ShaderHandleTyped<S>,
        elapsed_time: f32,
    ) -> DrawCommand {
        self.transform = (self.update)(elapsed_time, self.transform);
        DrawCommand {
            shader: shader.into(),
            model: self.model.into(),
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
    type Next: DrawableTypeList;
}

impl DrawableTypeList for Nil {
    const LEN: usize = 0;
    type Next = Self;
}

pub struct DrawableContainer<
    S: ShaderType
> {
    shader: ShaderHandleTyped<S>,
    objects: Vec<Object<S::Vertex, S::Material>>,
}

impl<
        S: ShaderType,
        N: DrawableTypeList,
    > DrawableTypeList for Cons<DrawableContainer<S>, N>
{
    const LEN: usize = N::LEN + 1;
    type Next = N;
}

pub trait DrawableCollection: DrawableTypeList {
    fn update(&mut self, elapsed_time: f32, draw_commands: &mut Vec<DrawCommand>);
}

impl DrawableCollection for Nil {
    fn update(&mut self, _elapsed_time: f32, _draw_commands: &mut  Vec<DrawCommand>) {}
}

impl<
        S: ShaderType,
        N: DrawableCollection,
    > DrawableCollection for Cons<DrawableContainer<S>, N>
{
    fn update(&mut self, elapsed_time: f32, draw_commands: &mut Vec<DrawCommand>) {
        draw_commands.extend(self
            .head
            .objects
            .iter_mut()
            .map(|object| object.update(self.head.shader, elapsed_time))
        );
        N::update(&mut self.tail, elapsed_time, draw_commands);
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
    >(
        self,
        shader: ShaderHandleTyped<S>,
        objects: Vec<Object<S::Vertex, S::Material>>,
    ) -> Scene<Cons<DrawableContainer<S>, D>, B> {
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
        let mut context = create_context(&mut renderer, scene.renderer_context)?;
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
        let mut draw_commands = vec![];
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
                    scene.objects.update(elapsed_time, &mut draw_commands);
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
                    std::mem::take(&mut draw_commands).into_iter().for_each(|command| {
                        context.draw(command.shader, command.model, &command.transform).unwrap();
                    });
                    let _ = context.end_frame();
                }
                _ => (),
            }
        })?;
        Ok(())
    }
}
