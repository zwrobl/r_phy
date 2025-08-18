use std::ops::Deref;

use graphics::renderer::{Context, DrawMapper, RendererContext};
use math::types::Vector2;
use type_kit::{Cons, Nil, list_type, list_value};
use winit::{event::WindowEvent, event_loop::EventLoopWindowTarget, window::Window};

use crate::system::{
    command::{CommandQueue, CommandSystem},
    frame::FrameData,
    input::InputSystem,
    renderer::{CameraCell, DrawQueue, RenderingSystem},
};

pub mod command;
pub mod control;
pub mod frame;
pub mod input;
pub mod renderer;

pub type SharedSystemsList = list_type![
    FrameData,
    InputSystem,
    CommandQueue,
    DrawQueue,
    CameraCell,
    Nil
];

pub struct ApplicationSystems<R: RendererContext, M: DrawMapper> {
    window: Window,
    command: CommandSystem,
    renderer: RenderingSystem<R, M>,
}

pub struct SharedSystems {
    systems: SharedSystemsList,
}

impl Deref for SharedSystems {
    type Target = SharedSystemsList;

    fn deref(&self) -> &Self::Target {
        &self.systems
    }
}

impl SharedSystems {
    pub fn begin_frame(&mut self, delta_time: f32) {
        self.systems
            .get_mut::<FrameData, _>()
            .set_delta_time(delta_time);
    }

    pub fn register_events(&mut self, events: Vec<WindowEvent>) {
        self.systems
            .get_mut::<InputSystem, _>()
            .register_events(&events);
    }
}

impl<R: RendererContext, M: DrawMapper> ApplicationSystems<R, M> {
    pub fn new(window: Window, renderer: Context<R, M>) -> (Self, SharedSystems) {
        let (draw_queue, renderer) = RenderingSystem::new(renderer);
        let (command_queue, command_system) = CommandSystem::new();
        let frame_data = FrameData::new(Vector2::new(
            window.inner_size().width as f32,
            window.inner_size().height as f32,
        ));
        let input_system = InputSystem::new();
        let camera_cell = CameraCell::new();
        (
            Self {
                window,
                renderer,
                command: command_system,
            },
            SharedSystems {
                systems: list_value![
                    frame_data,
                    input_system,
                    command_queue,
                    draw_queue,
                    camera_cell,
                    Nil::new()
                ],
            },
        )
    }

    pub fn process(&mut self, shared: &mut SharedSystems, elwt: &EventLoopWindowTarget<()>) {
        self.renderer
            .process(shared.systems.get_mut::<CameraCell, _>());
        self.command.process(
            shared.systems.get_mut::<InputSystem, _>(),
            &self.window,
            elwt,
        );
    }
}
