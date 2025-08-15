pub mod first_person;

use bytemuck::{Pod, Zeroable};
use input::InputSystem;
use math::types::{Matrix4, Vector3};

pub const UP: Vector3 = Vector3::z();

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct CameraMatrices {
    pub view: Matrix4,
    pub proj: Matrix4,
}

pub trait Camera: 'static {
    fn get_position(&self) -> Vector3;
    fn get_matrices(&self) -> CameraMatrices;
    fn update(&mut self, elapsed_time: f32, input_system: &InputSystem);
    fn set_active(&mut self, active: bool);
}
pub struct CameraNone;

impl Camera for CameraNone {
    fn get_position(&self) -> Vector3 {
        unimplemented!()
    }

    fn get_matrices(&self) -> CameraMatrices {
        unimplemented!()
    }

    fn update(&mut self, _elapsed_time: f32, _input_system: &InputSystem) {
        unimplemented!()
    }

    fn set_active(&mut self, _active: bool) {
        unimplemented!()
    }
}
