use std::sync::mpsc::{Receiver, Sender};

use math::types::Vector2;
use winit::{
    dpi::PhysicalPosition,
    event_loop::EventLoopWindowTarget,
    window::{CursorGrabMode, Window},
};

use crate::system::input::InputSystem;

pub enum Command {
    Quit,
    LockCursor,
    UnlockCursor,
}

pub struct CommandSystem {
    cursor_state: CursorState,
    receiver: Receiver<Command>,
}

pub struct CommandQueue {
    sender: Sender<Command>,
}

#[derive(Debug, Clone, Copy)]
enum CursorState {
    Locked,
    Free,
}

impl CursorState {
    pub fn unlock(&mut self, window: &Window) {
        if matches!(self, Self::Locked) {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            *self = Self::Free;
        }
    }

    pub fn lock(&mut self, window: &Window) {
        if matches!(self, Self::Free) {
            let _ = window.set_cursor_grab(CursorGrabMode::Confined);
            window.set_cursor_visible(false);
            *self = Self::Locked;
        }
    }
}

impl CommandSystem {
    pub fn new() -> (CommandQueue, Self) {
        let (sender, receiver) = std::sync::mpsc::channel();
        (
            CommandQueue { sender },
            Self {
                cursor_state: CursorState::Free,
                receiver,
            },
        )
    }

    pub fn process(
        &mut self,
        input_system: &mut InputSystem,
        window: &Window,
        elwt: &EventLoopWindowTarget<()>,
    ) {
        self.receiver.try_iter().for_each(|command| match command {
            Command::Quit => elwt.exit(),
            Command::LockCursor => {
                self.cursor_state.lock(window);
            }
            Command::UnlockCursor => {
                self.cursor_state.unlock(window);
            }
        });
        if matches!(self.cursor_state, CursorState::Locked) {
            let window_extent = window.inner_size();
            let _ = window.set_cursor_position(PhysicalPosition {
                x: window_extent.width / 2,
                y: window_extent.height / 2,
            });
            input_system.set_cursor_position(Vector2::new(
                window_extent.width as f32 / 2.0,
                window_extent.height as f32 / 2.0,
            ));
        }
    }
}

impl CommandQueue {
    pub fn send(&self, command: Command) {
        self.sender.send(command).unwrap();
    }
}
