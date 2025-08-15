use std::f32::consts::{FRAC_PI_2, PI};

use math::types::{Matrix4, Vector2, Vector3};

use crate::renderer::camera::UP;
use input::{InputSystem, Key};

use super::{Camera, CameraMatrices};

impl Camera for FirstPersonCamera {
    fn get_position(&self) -> Vector3 {
        self.position
    }

    fn get_matrices(&self) -> CameraMatrices {
        self.into()
    }

    fn update(&mut self, elapsed_time: f32, input_system: &InputSystem) {
        const MOUSE_SENSITIVITY: f32 = 0.5;
        const MOVEMENT_SPEED: f32 = 4.0;

        if !self.active {
            return;
        }

        let cursor = input_system.get_cursor_position();
        let delta_x = cursor.x - self.screen_center.x;
        let delta_y = cursor.y - self.screen_center.y;
        let delta_yaw = (delta_x / self.screen_center.x) as f32 * MOUSE_SENSITIVITY;
        let delta_pitch = (delta_y / self.screen_center.y) as f32 * MOUSE_SENSITIVITY;
        self.euler.y = (self.euler.y + delta_pitch).clamp(-FRAC_PI_2 + 1e-4, FRAC_PI_2 - 1e-4);
        self.euler.x = ((self.euler.x - delta_yaw) / (2.0 * PI)).fract() * (2.0 * PI);
        self.forward = Vector3::from_euler(self.euler.x, self.euler.y, self.euler.z);
        self.right = self.forward.cross(UP).norm();

        if input_system.get_key_state(Key::W).is_pressed() {
            self.move_direction = self.move_direction + self.forward;
        }

        if input_system.get_key_state(Key::S).is_pressed() {
            self.move_direction = self.move_direction - self.forward;
        }

        if input_system.get_key_state(Key::D).is_pressed() {
            self.move_direction = self.move_direction + self.right;
        }

        if input_system.get_key_state(Key::A).is_pressed() {
            self.move_direction = self.move_direction - self.right;
        }

        if self.move_direction.length_square() > 0.0 {
            self.position =
                self.position + elapsed_time * MOVEMENT_SPEED * self.move_direction.norm();
        }
        self.forward = Vector3::from_euler(self.euler.x, self.euler.y, self.euler.z);
        self.right = self.forward.cross(UP).norm();
        self.move_direction = Vector3::zero();
    }

    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
}

impl From<&FirstPersonCamera> for CameraMatrices {
    fn from(value: &FirstPersonCamera) -> Self {
        CameraMatrices {
            proj: value.proj,
            view: Matrix4::look_at(value.position, value.position + value.forward, UP),
        }
    }
}

pub struct FirstPersonCamera {
    proj: Matrix4,
    position: Vector3,
    forward: Vector3,
    right: Vector3,
    euler: Vector3,
    move_direction: Vector3,
    active: bool,
    screen_center: Vector2,
}

impl FirstPersonCamera {
    pub fn new(proj: Matrix4, resolution: Vector2) -> Self {
        Self {
            proj,
            position: Vector3::zero(),
            forward: Vector3::x(),
            right: -Vector3::y(),
            euler: Vector3::zero(),
            move_direction: Vector3::zero(),
            active: false,
            screen_center: resolution / 2.0,
        }
    }

    pub fn with_position(mut self, position: Vector3) -> Self {
        self.position = position;
        self
    }

    pub fn look_at(mut self, target: Vector3) -> Self {
        self.forward = (target - self.position).norm();
        self.right = self.forward.cross(UP).norm();
        self.euler = Vector3::from_euler(self.forward.x, self.forward.y, 0.0);
        self
    }
}
