pub mod error;
pub mod system;

use entity::{context::EntityComponentContext, entity::EntityBuilder, EntityComponentSystem};
use input::{InputSystem, Key, KeyState};
use math::types::Vector2;
use type_kit::{list_type, list_value, Cons, Nil};
use winit::{
    dpi::PhysicalPosition,
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

use std::{marker::PhantomData, time::Instant};

use graphics::renderer::{camera::CameraNone, create_context};

use graphics::renderer::{camera::Camera, ContextBuilder, Renderer, RendererBuilder};

use crate::{
    error::{SystemError, SystemResult},
    system::{
        frame::FrameData,
        renderer::{DrawCommandChannel, DrawQueue},
    },
};

#[derive(Debug, Clone, Copy)]
enum CursorState {
    Locked,
    Free,
}

impl CursorState {
    pub fn new() -> Self {
        Self::Free
    }
    pub fn switch(&mut self, window: &Window) -> SystemResult<()> {
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

pub struct LoopBuilder<R: RendererBuilder, C: Camera> {
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

impl<R: RendererBuilder, C: Camera> LoopBuilder<R, C> {
    pub fn with_window(self, window: WindowBuilder) -> Self {
        Self {
            window: Some(window),
            ..self
        }
    }

    pub fn with_camera<N: Camera>(self, camera: N) -> LoopBuilder<R, N> {
        let Self {
            window, renderer, ..
        } = self;
        LoopBuilder {
            camera: Some(camera),
            window,
            renderer,
        }
    }

    pub fn build(self) -> SystemResult<Loop<impl Renderer, C>> {
        let Self {
            window,
            renderer,
            camera,
        } = self;
        let event_loop = EventLoop::new()?;
        let window = window
            .ok_or(SystemError::MissingWindowConfiguration)?
            .build(&event_loop)?;
        let renderer = renderer.build(&window)?;
        let camera = camera.ok_or(SystemError::MissingCameraConfiguration)?;
        let (draw_queue, draw_storage) = DrawCommandChannel::new();
        let external = list_value![
            InputSystem::new(),
            draw_queue,
            FrameData::new(0.0),
            Nil::new()
        ];
        Ok(Loop {
            event_loop,
            window,
            renderer,
            camera,
            external,
            draw_storage,
        })
    }
}

pub struct Loop<R: Renderer, C: Camera> {
    renderer: R,
    window: Window,
    event_loop: EventLoop<()>,
    external: ExternalSystems,
    draw_storage: DrawCommandChannel,
    camera: C,
}

pub trait LoopTypes {
    type Renderer: Renderer;
    type Camera: Camera;
}

impl<R: Renderer, C: Camera> LoopTypes for Loop<R, C> {
    type Renderer = R;
    type Camera = C;
}

pub type ExternalSystems = list_type![InputSystem, DrawQueue, FrameData, Nil];

pub struct Scene<
    E: EntityComponentContext,
    D: EntityComponentSystem<E, ExternalSystems>,
    B: ContextBuilder,
> {
    renderer_context: B,
    entity_context: D,
    _phantom: PhantomData<E>,
}

impl<
        E: EntityComponentContext,
        D: EntityComponentSystem<E, ExternalSystems>,
        B: ContextBuilder,
    > Scene<E, D, B>
{
    pub fn get_entity_builder(&self) -> EntityBuilder<E> {
        self.entity_context.get_entity_builder()
    }

    pub fn with_entity(&mut self, entity: EntityBuilder<E>) -> &mut Self {
        self.entity_context.add_entity(entity);
        self
    }
}

impl<R: Renderer, C: Camera> Loop<R, C> {
    pub fn renderer_context_builder(&self) -> impl ContextBuilder<Renderer = R> {
        R::context_builder()
    }

    pub fn system_builder<'a, E: EntityComponentContext>(
        &'a self,
    ) -> impl entity::stage::Builder<E, ExternalSystems> {
        E::with_external()
    }

    pub fn scene<
        E: EntityComponentContext,
        S: entity::stage::Builder<E, ExternalSystems>,
        B: ContextBuilder<Renderer = R>,
    >(
        &self,
        builder: B,
        systems: S,
    ) -> Scene<E, impl EntityComponentSystem<E, ExternalSystems>, B> {
        Scene {
            renderer_context: builder,
            entity_context: systems.build(),
            _phantom: PhantomData::<E>,
        }
    }

    pub fn run<
        E: EntityComponentContext,
        D: EntityComponentSystem<E, ExternalSystems>,
        B: ContextBuilder<Renderer = R>,
    >(
        self,
        scene: Scene<E, D, B>,
    ) -> SystemResult<()> {
        let Self {
            window,
            event_loop,
            mut renderer,
            mut camera,
            mut external,
            draw_storage,
        } = self;
        let Scene {
            renderer_context,
            mut entity_context,
            ..
        } = scene;
        let mut renderer_context = create_context(&mut renderer, renderer_context)?;
        let mut cursor_state = CursorState::new();
        let mut window_events = vec![];
        let mut previous_frame_time = Instant::now();
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run(|event, elwt| match event {
            Event::NewEvents(StartCause::Poll) => {
                let current_frame_time = Instant::now();
                let elapsed_time = (current_frame_time - previous_frame_time).as_secs_f32();
                previous_frame_time = current_frame_time;
                external
                    .get_mut::<FrameData, _>()
                    .set_delta_time(elapsed_time);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                elwt.exit();
            }
            Event::WindowEvent { event, .. } => {
                window_events.push(event);
            }
            Event::AboutToWait => {
                external
                    .get_mut::<InputSystem, _>()
                    .register_events(&std::mem::take(&mut window_events));
                camera.update(
                    external.get::<FrameData, _>().delta_time(),
                    external.get::<InputSystem, _>(),
                );
                entity_context.execute_systems(&external);
                let _ = renderer_context.begin_frame(&camera);
                draw_storage.receive().into_iter().for_each(|command| {
                    renderer_context
                        .draw(command.shader, command.model, &command.transform)
                        .unwrap();
                });
                let _ = renderer_context.end_frame();
                if external
                    .get::<InputSystem, _>()
                    .get_key_state(Key::G)
                    .matches_state(KeyState::Pressed)
                {
                    let _ = cursor_state.switch(&window);
                    match cursor_state {
                        CursorState::Free => camera.set_active(false),
                        CursorState::Locked => camera.set_active(true),
                    }
                }
                if external
                    .get::<InputSystem, _>()
                    .get_key_state(Key::Q)
                    .matches_state(KeyState::Pressed)
                {
                    elwt.exit();
                }
                if let CursorState::Locked = cursor_state {
                    let window_extent = window.inner_size();
                    let _ = window.set_cursor_position(PhysicalPosition {
                        x: window_extent.width / 2,
                        y: window_extent.height / 2,
                    });
                    external
                        .get_mut::<InputSystem, _>()
                        .set_cursor_position(Vector2::new(
                            window_extent.width as f32 / 2.0,
                            window_extent.height as f32 / 2.0,
                        ));
                }
            }
            _ => (),
        })?;
        Ok(())
    }
}
