use std::{f32::consts::FRAC_PI_2, marker::PhantomData, sync::Mutex};

use entity::{
    context::{EntityComponentContext, EntityUpdateType},
    entity::ComponentUpdate,
    system::System,
};
use graphics::renderer::camera::UP;
use math::{
    transform::Transform,
    types::{Matrix3, Quat, Vector3},
};

use type_kit::{list_type, unpack_list, Cons, Marker, Nil, RefList, UContains};

use crate::system::{
    command::{self, CommandQueue},
    frame::FrameData,
    input::{InputSystem, Key, KeyState},
};

pub struct FirstPerson<M: Marker> {
    is_active: Mutex<bool>,
    _marker: PhantomData<M>,
}

impl<M: Marker> FirstPerson<M> {
    pub fn update(
        &self,
        input_system: &InputSystem,
        key_bindings: &KeyBindings,
        command_queue: &CommandQueue,
    ) -> bool {
        let mut is_active = self.is_active.lock().unwrap();
        if input_system
            .get_key_state(key_bindings.focus)
            .matches_state(KeyState::Pressed)
        {
            *is_active = !*is_active;
            match *is_active {
                true => command_queue.send(command::Command::LockCursor),
                false => command_queue.send(command::Command::UnlockCursor),
            }
        }
        *is_active
    }
}

pub struct KeyBindings {
    forward: Key,
    backward: Key,
    left: Key,
    right: Key,
    focus: Key,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            forward: Key::W,
            backward: Key::S,
            left: Key::A,
            right: Key::D,
            focus: Key::G,
        }
    }
}

impl KeyBindings {
    pub fn new(forward: Key, backward: Key, left: Key, right: Key, focus: Key) -> Self {
        Self {
            forward,
            backward,
            left,
            right,
            focus,
        }
    }
}

pub struct FirstPersonController {
    key_bindings: KeyBindings,
    movement_speed: f32,
    mouse_sensitivity: f32,
}

impl FirstPersonController {
    pub fn new(key_bindings: KeyBindings, movement_speed: f32, mouse_sensitivity: f32) -> Self {
        Self {
            key_bindings,
            movement_speed,
            mouse_sensitivity,
        }
    }

    fn get_euler_delta(&self, input_system: &InputSystem, frame_data: &FrameData) -> Vector3 {
        let cursor = input_system.get_cursor_position();
        let screen_center = frame_data.screen_center();
        let delta_x = cursor.x - screen_center.x;
        let delta_y = cursor.y - screen_center.y;
        Vector3::new(
            0.0,
            (delta_y / screen_center.y) * self.mouse_sensitivity,
            (-delta_x / screen_center.x) * self.mouse_sensitivity,
        )
    }

    fn get_translation_delta(
        &self,
        transform: &Transform,
        input_system: &InputSystem,
        frame_data: &FrameData,
    ) -> Vector3 {
        let mat: Matrix3 = transform.q.into();
        let forward = mat.i;
        let right = forward.cross(UP).norm();

        let mut move_direction = Vector3::zero();
        if input_system
            .get_key_state(self.key_bindings.forward)
            .is_pressed()
        {
            move_direction = move_direction + forward;
        }

        if input_system
            .get_key_state(self.key_bindings.backward)
            .is_pressed()
        {
            move_direction = move_direction - forward;
        }

        if input_system
            .get_key_state(self.key_bindings.right)
            .is_pressed()
        {
            move_direction = move_direction + right;
        }

        if input_system
            .get_key_state(self.key_bindings.left)
            .is_pressed()
        {
            move_direction = move_direction - right;
        }

        if move_direction.length_square() > 0.0 {
            frame_data.delta_time() * self.movement_speed * move_direction.norm()
        } else {
            Vector3::zero()
        }
    }
}

impl<M: Marker> FirstPerson<M> {
    pub fn new<E: EntityComponentContext>() -> Self
    where
        EntityUpdateType<E>: UContains<ComponentUpdate<Transform>, M>,
    {
        Self {
            is_active: Mutex::new(false),
            _marker: PhantomData,
        }
    }
}

impl<E: EntityComponentContext, M: Marker> System<E> for FirstPerson<M>
where
    EntityUpdateType<E>: UContains<ComponentUpdate<Transform>, M>,
{
    type External = list_type![InputSystem, FrameData, CommandQueue, Nil];
    type WriteList = list_type![Transform, Nil];
    type Components = list_type![Transform, FirstPersonController, Nil];

    fn execute<'a>(
        &self,
        entity: entity::index::EntityIndex,
        unpack_list![transform, controller]: RefList<'a, Self::Components>,
        _context: &E,
        queue: &entity::operation::OperationSender<E>,
        unpack_list![input_system, frame_data, command_queue]: RefList<'a, Self::External>,
    ) {
        if self.update(input_system, &controller.key_bindings, command_queue) {
            let euler_delta = controller.get_euler_delta(input_system, frame_data);
            let translation_delta =
                controller.get_translation_delta(transform, input_system, frame_data);

            let mut euler = transform.q.to_euler() + euler_delta;
            euler.y = euler.y.clamp(-FRAC_PI_2 + 1e-3, FRAC_PI_2 - 1e-3);
            let q = Quat::from_euler(euler);

            let transform = Transform::new(q, transform.t + translation_delta);
            let entity = entity.in_context::<E>();
            let update = self.get_entity_update(entity, ComponentUpdate::update(transform));
            queue.update_entity(update);
        }
    }
}
