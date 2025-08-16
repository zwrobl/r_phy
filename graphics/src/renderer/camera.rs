use bytemuck::{Pod, Zeroable};
use math::{
    transform::Transform,
    types::{Matrix3, Matrix4, Vector3},
};

pub const UP: Vector3 = Vector3::z();

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct CameraMatrices {
    pub view: Matrix4,
    pub proj: Matrix4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct ViewMatrix {
    pub view: Matrix4,
}

impl From<Transform> for ViewMatrix {
    fn from(value: Transform) -> Self {
        let mat: Matrix3 = value.q.into();
        let forward = mat.i.norm();
        let view = Matrix4::look_at(value.t, value.t + forward, UP);
        Self { view }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct ProjectionMatrix {
    pub proj: Matrix4,
}

impl ProjectionMatrix {
    pub fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> Self {
        let proj = Matrix4::perspective(fov, aspect, near, far);
        Self { proj }
    }

    pub fn with_view(&self, view: ViewMatrix) -> CameraMatrices {
        CameraMatrices {
            view: view.view,
            proj: self.proj,
        }
    }
}
