use bytemuck::{Pod, Zeroable};
use math::{
    transform::Transform,
    types::{Matrix4, Vector3},
};

pub const UP: Vector3 = Vector3::z();

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct CameraMatrices {
    pub view: Matrix4,
    pub proj: Matrix4,
}

pub struct ViewMatrix {
    pub view: Matrix4,
}

impl From<Transform> for ViewMatrix {
    fn from(value: Transform) -> Self {
        let forward = (value.q * Vector3::x()).norm();
        let view = Matrix4::look_at(value.t, value.t + forward, UP);
        Self { view }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectionMatrix {
    pub proj: Matrix4,
}

impl ProjectionMatrix {
    pub fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> Self {
        let proj = Matrix4::perspective(fov, aspect, near, far);
        Self { proj }
    }

    pub fn with_view(&self, view: Transform) -> Camera {
        Camera { view, proj: *self }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub view: Transform,
    pub proj: ProjectionMatrix,
}

impl From<Camera> for CameraMatrices {
    fn from(camera: Camera) -> Self {
        let ViewMatrix { view } = camera.view.into();
        let ProjectionMatrix { proj } = camera.proj;
        Self { view, proj }
    }
}
