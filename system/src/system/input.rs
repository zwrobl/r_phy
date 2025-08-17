use std::marker::PhantomData;

use entity::{context::EntityComponentContext, operation::OperationChannel, system::GlobalSystem};
use type_kit::{list_type, unpack_list, Cons, Nil};

use crate::system::command::CommandQueue;

use math::types::Vector2;
use strum::EnumCount;
use winit::{
    event::{self, MouseButton},
    keyboard::{KeyCode, PhysicalKey},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display, strum::EnumCount)]
pub enum KeyState {
    Pressed,
    Hold,
    Released,
    None,
}

impl KeyState {
    fn next_state(&mut self) {
        *self = match self {
            KeyState::Pressed => KeyState::Hold,
            KeyState::Released => KeyState::None,
            _ => *self,
        }
    }

    fn update_state(&mut self, element_state: event::ElementState) {
        *self = match self {
            KeyState::Hold | KeyState::Pressed
                if matches!(element_state, event::ElementState::Released) =>
            {
                KeyState::Released
            }
            KeyState::None | KeyState::Released
                if matches!(element_state, event::ElementState::Pressed) =>
            {
                KeyState::Pressed
            }
            _ => *self,
        }
    }

    pub fn is_pressed(&self) -> bool {
        matches!(self, KeyState::Pressed | KeyState::Hold)
    }

    pub fn matches_state(&self, key_state: KeyState) -> bool {
        *self == key_state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display, strum::EnumCount)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    CtrlLeft,
    ShiftLeft,
    AltLeft,
    CtrlRight,
    ShiftRight,
    AltRight,
    Space,
    Enter,
    Backspace,
    CapsLock,
    Tab,
    Backquote,
    PageUp,
    PageDown,
    Home,
    End,
    Insert,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
    MouseLeft,
    MouseRight,
    MouseMiddle,
    MouseBack,
    MouseForward,
}

impl Key {
    fn from_mouse_button(button: MouseButton) -> Option<Self> {
        match button {
            MouseButton::Left => Some(Key::MouseLeft),
            MouseButton::Right => Some(Key::MouseRight),
            MouseButton::Middle => Some(Key::MouseMiddle),
            MouseButton::Back => Some(Key::MouseBack),
            MouseButton::Forward => Some(Key::MouseForward),
            _ => None,
        }
    }

    fn from_key_code(key_code: KeyCode) -> Option<Key> {
        match key_code {
            KeyCode::KeyA => Some(Key::A),
            KeyCode::KeyB => Some(Key::B),
            KeyCode::KeyC => Some(Key::C),
            KeyCode::KeyD => Some(Key::D),
            KeyCode::KeyE => Some(Key::E),
            KeyCode::KeyF => Some(Key::F),
            KeyCode::KeyG => Some(Key::G),
            KeyCode::KeyH => Some(Key::H),
            KeyCode::KeyI => Some(Key::I),
            KeyCode::KeyJ => Some(Key::J),
            KeyCode::KeyK => Some(Key::K),
            KeyCode::KeyL => Some(Key::L),
            KeyCode::KeyM => Some(Key::M),
            KeyCode::KeyN => Some(Key::N),
            KeyCode::KeyO => Some(Key::O),
            KeyCode::KeyP => Some(Key::P),
            KeyCode::KeyQ => Some(Key::Q),
            KeyCode::KeyR => Some(Key::R),
            KeyCode::KeyS => Some(Key::S),
            KeyCode::KeyT => Some(Key::T),
            KeyCode::KeyU => Some(Key::U),
            KeyCode::KeyV => Some(Key::V),
            KeyCode::KeyW => Some(Key::W),
            KeyCode::KeyX => Some(Key::X),
            KeyCode::KeyY => Some(Key::Y),
            KeyCode::KeyZ => Some(Key::Z),
            KeyCode::Digit1 => Some(Key::Key1),
            KeyCode::Digit2 => Some(Key::Key2),
            KeyCode::Digit3 => Some(Key::Key3),
            KeyCode::Digit4 => Some(Key::Key4),
            KeyCode::Digit5 => Some(Key::Key5),
            KeyCode::Digit6 => Some(Key::Key6),
            KeyCode::Digit7 => Some(Key::Key7),
            KeyCode::Digit8 => Some(Key::Key8),
            KeyCode::Digit9 => Some(Key::Key9),
            KeyCode::Digit0 => Some(Key::Key0),
            KeyCode::ControlLeft => Some(Key::CtrlLeft),
            KeyCode::ShiftLeft => Some(Key::ShiftLeft),
            KeyCode::AltLeft => Some(Key::AltLeft),
            KeyCode::ControlRight => Some(Key::CtrlRight),
            KeyCode::ShiftRight => Some(Key::ShiftRight),
            KeyCode::AltRight => Some(Key::AltRight),
            KeyCode::Space => Some(Key::Space),
            KeyCode::Enter => Some(Key::Enter),
            KeyCode::Backspace => Some(Key::Backspace),
            KeyCode::CapsLock => Some(Key::CapsLock),
            KeyCode::Tab => Some(Key::Tab),
            KeyCode::Backquote => Some(Key::Backquote),
            KeyCode::PageUp => Some(Key::PageUp),
            KeyCode::PageDown => Some(Key::PageDown),
            KeyCode::Home => Some(Key::Home),
            KeyCode::End => Some(Key::End),
            KeyCode::Insert => Some(Key::Insert),
            KeyCode::Delete => Some(Key::Delete),
            KeyCode::ArrowLeft => Some(Key::ArrowLeft),
            KeyCode::ArrowRight => Some(Key::ArrowRight),
            KeyCode::ArrowUp => Some(Key::ArrowUp),
            KeyCode::ArrowDown => Some(Key::ArrowDown),
            KeyCode::Minus => Some(Key::Minus),
            KeyCode::Equal => Some(Key::Equal),
            KeyCode::BracketLeft => Some(Key::BracketLeft),
            KeyCode::BracketRight => Some(Key::BracketRight),
            KeyCode::Backslash => Some(Key::Backslash),
            KeyCode::Semicolon => Some(Key::Semicolon),
            KeyCode::Quote => Some(Key::Quote),
            KeyCode::Comma => Some(Key::Comma),
            KeyCode::Period => Some(Key::Period),
            KeyCode::Slash => Some(Key::Slash),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CursorState {
    position: Vector2,
    delta: Vector2,
}

impl CursorState {
    pub fn new(position: Vector2) -> Self {
        Self {
            position,
            delta: Vector2::zero(),
        }
    }

    pub fn update_position(&mut self, new_position: Vector2) {
        self.delta = new_position - self.position;
        self.position = new_position;
    }

    pub fn get_position(&self) -> Vector2 {
        self.position
    }

    pub fn get_delta(&self) -> Vector2 {
        self.delta
    }
}

pub struct InputSystem {
    key_state: [KeyState; Key::COUNT],
    cursor: CursorState,
}

impl InputSystem {
    pub fn new() -> Self {
        Self {
            key_state: [KeyState::None; Key::COUNT],
            cursor: CursorState::default(),
        }
    }

    pub fn get_key_state(&self, key: Key) -> KeyState {
        self.key_state[key as usize]
    }

    pub fn get_cursor_position(&self) -> Vector2 {
        self.cursor.get_position()
    }

    pub fn get_cursor_delta(&self) -> Vector2 {
        self.cursor.get_delta()
    }

    pub fn set_cursor_position(&mut self, position: Vector2) {
        self.cursor = CursorState::new(position);
    }

    pub fn register_events(&mut self, events: &[event::WindowEvent]) {
        self.update_key_states();
        events.iter().for_each(|event| match event {
            event::WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = Key::from_mouse_button(*button) {
                    self.key_state[button as usize].update_state(*state);
                }
            }
            event::WindowEvent::CursorMoved { position, .. } => {
                self.cursor
                    .update_position(Vector2::new(position.x as f32, position.y as f32));
            }
            event::WindowEvent::KeyboardInput {
                event:
                    event::KeyEvent {
                        physical_key: PhysicalKey::Code(key_code),
                        state,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                if let Some(key) = Key::from_key_code(*key_code) {
                    self.key_state[key as usize].update_state(*state);
                }
            }
            _ => {}
        })
    }

    fn update_key_states(&mut self) {
        self.key_state.iter_mut().for_each(|event| {
            event.next_state();
        })
    }
}

pub struct GlobalInput<
    E: EntityComponentContext,
    F: Fn(&E, &OperationChannel<'_, E>, &InputSystem, &CommandQueue) + Send + Sync,
> {
    pub update_fn: F,
    _phantom: PhantomData<E>,
}

impl<
        E: EntityComponentContext,
        F: Fn(&E, &OperationChannel<'_, E>, &InputSystem, &CommandQueue) + Send + Sync,
    > GlobalInput<E, F>
{
    pub fn new(update_fn: F) -> Self {
        Self {
            update_fn,
            _phantom: PhantomData,
        }
    }

    pub fn update(
        &self,
        context: &E,
        queue: &OperationChannel<'_, E>,
        input_system: &InputSystem,
        command_queue: &CommandQueue,
    ) {
        (self.update_fn)(context, queue, input_system, command_queue)
    }
}

impl<
        E: EntityComponentContext,
        F: Fn(&E, &OperationChannel<'_, E>, &InputSystem, &CommandQueue) + Send + Sync,
    > GlobalSystem<E> for GlobalInput<E, F>
{
    type External = list_type![InputSystem, CommandQueue, Nil];
    type WriteList = Nil;

    fn execute<'a>(
        &self,
        context: &E,
        queue: &OperationChannel<'_, E>,
        unpack_list![input_system, command_queue]: type_kit::RefList<'a, Self::External>,
    ) {
        self.update(context, queue, input_system, command_queue);
    }
}
