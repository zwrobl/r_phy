pub mod error;
pub mod system;

use entity::{context::EntityComponentContext, entity::EntityBuilder, EntityComponentSystem};
use winit::{
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

use std::{marker::PhantomData, time::Instant};

use graphics::renderer::{create_context, ContextBuilder, Renderer, RendererBuilder};

use crate::{
    error::{SystemError, SystemResult},
    system::{ApplicationSystems, SharedSystemsList},
};

pub struct LoopBuilder<R: RendererBuilder> {
    renderer: R,
    window: Option<WindowBuilder>,
}

impl<N: RendererBuilder> LoopBuilder<N> {
    pub fn new(renderer: N) -> Self {
        Self {
            window: None,
            renderer,
        }
    }
}

impl<R: RendererBuilder> LoopBuilder<R> {
    pub fn with_window(self, window: WindowBuilder) -> Self {
        Self {
            window: Some(window),
            ..self
        }
    }

    pub fn build(self) -> SystemResult<Loop<impl Renderer>> {
        let Self { window, renderer } = self;
        let event_loop = EventLoop::new()?;
        let window = window
            .ok_or(SystemError::MissingWindowConfiguration)?
            .build(&event_loop)?;
        let renderer = renderer.build(&window)?;
        Ok(Loop {
            event_loop,
            window,
            renderer,
        })
    }
}

pub struct Loop<R: Renderer> {
    renderer: R,
    window: Window,
    event_loop: EventLoop<()>,
}

pub struct Scene<
    E: EntityComponentContext,
    D: EntityComponentSystem<E, SharedSystemsList>,
    B: ContextBuilder,
> {
    renderer_context: B,
    entity_context: D,
    _phantom: PhantomData<E>,
}

impl<
        E: EntityComponentContext,
        D: EntityComponentSystem<E, SharedSystemsList>,
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

impl<R: Renderer> Loop<R> {
    pub fn renderer_context_builder(&self) -> impl ContextBuilder<Renderer = R> {
        R::context_builder()
    }

    pub fn system_builder<'a, E: EntityComponentContext>(
        &'a self,
    ) -> impl entity::stage::Builder<E, SharedSystemsList> {
        E::with_external()
    }

    pub fn scene<
        E: EntityComponentContext,
        S: entity::stage::Builder<E, SharedSystemsList>,
        B: ContextBuilder<Renderer = R>,
    >(
        &self,
        builder: B,
        systems: S,
    ) -> Scene<E, impl EntityComponentSystem<E, SharedSystemsList>, B> {
        Scene {
            renderer_context: builder,
            entity_context: systems.build(),
            _phantom: PhantomData::<E>,
        }
    }

    pub fn run<
        E: EntityComponentContext,
        D: EntityComponentSystem<E, SharedSystemsList>,
        B: ContextBuilder<Renderer = R>,
    >(
        self,
        scene: Scene<E, D, B>,
    ) -> SystemResult<()> {
        let Self {
            event_loop,
            mut renderer,
            window,
        } = self;
        let Scene {
            renderer_context,
            mut entity_context,
            ..
        } = scene;
        let (mut global_systems, mut shared) =
            ApplicationSystems::new(window, create_context(&mut renderer, renderer_context)?);
        let mut window_events = vec![];
        let mut previous_frame_time = Instant::now();
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop.run(|event, elwt| match event {
            Event::NewEvents(StartCause::Poll) => {
                let current_frame_time = Instant::now();
                let delta_time = (current_frame_time - previous_frame_time).as_secs_f32();
                previous_frame_time = current_frame_time;
                shared.begin_frame(delta_time);
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
                shared.register_events(std::mem::take(&mut window_events));
                entity_context.execute_systems(&shared);
                global_systems.process(&mut shared, elwt);
            }
            _ => (),
        })?;
        Ok(())
    }
}
