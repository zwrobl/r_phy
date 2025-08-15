mod gltf;
mod material;
mod mesh;

use std::fmt::Debug;

pub use material::*;
pub use mesh::*;
use type_kit::TypeGuardError;

#[derive(Debug)]
pub struct ModelTyped<M: Material, V: Vertex> {
    pub mesh: MeshHandleTyped<V>,
    pub material: MaterialHandleTyped<M>,
}

impl<M: Material, V: Vertex> Clone for ModelTyped<M, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: Material, V: Vertex> Copy for ModelTyped<M, V> {}

impl<M: Material, V: Vertex> ModelTyped<M, V> {
    pub fn new(mesh: MeshHandleTyped<V>, material: MaterialHandleTyped<M>) -> Self {
        Self { mesh, material }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Model {
    mesh: MeshHandle,
    material: MaterialHandle,
}

impl<V: Vertex, M: Material> From<ModelTyped<M, V>> for Model {
    fn from(model: ModelTyped<M, V>) -> Self {
        Self {
            mesh: model.mesh.into(),
            material: model.material.into(),
        }
    }
}

impl<V: Vertex, M: Material> TryFrom<Model> for ModelTyped<M, V> {
    type Error = TypeGuardError;

    fn try_from(model: Model) -> Result<Self, Self::Error> {
        Ok(ModelTyped {
            mesh: model.mesh.try_into()?,
            material: model.material.try_into()?,
        })
    }
}
