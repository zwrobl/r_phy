use std::{
    marker::PhantomData,
    path::{Path, PathBuf},
};

use crate::model::{EmptyMaterial, Material, Vertex, VertexNone};
use type_kit::{Cons, FromGuard, Nil, TypeGuard, TypeGuardError};

pub trait ShaderType: 'static {
    type Vertex: Vertex;
    type Material: Material;

    fn source(&self) -> &Path;
}

pub struct Shader<V: Vertex, M: Material> {
    source: PathBuf,
    _phantom: PhantomData<(V, M)>,
}

impl<V: Vertex, M: Material> Shader<V, M> {
    pub fn marker() -> PhantomData<Self> {
        PhantomData
    }

    pub fn new(source_path: &str) -> Self {
        Self {
            source: PathBuf::from(source_path),
            _phantom: PhantomData,
        }
    }
}

impl<V: Vertex, M: Material> ShaderType for Shader<V, M> {
    type Vertex = V;
    type Material = M;

    fn source(&self) -> &Path {
        &self.source
    }
}

pub trait ShaderTypeList: 'static {
    const LEN: usize;
    type Item: ShaderType;
    type Next: ShaderTypeList;
}

pub struct ShaderTypeNil {}

impl ShaderType for ShaderTypeNil {
    type Vertex = VertexNone;
    type Material = EmptyMaterial;

    fn source(&self) -> &Path {
        unreachable!()
    }
}

impl ShaderTypeList for Nil {
    const LEN: usize = 0;
    type Item = ShaderTypeNil;
    type Next = Self;
}

impl<S: ShaderType, N: ShaderTypeList> ShaderTypeList for Cons<Vec<S>, N> {
    const LEN: usize = N::LEN + 1;
    type Item = S;
    type Next = N;
}

#[derive(Debug)]
pub struct ShaderHandleTyped<S: ShaderType> {
    index: u32,
    _phantom: PhantomData<S>,
}

impl<S: ShaderType> Clone for ShaderHandleTyped<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: ShaderType> Copy for ShaderHandleTyped<S> {}

impl<S: ShaderType> ShaderHandleTyped<S> {
    pub fn new(index: u32) -> Self {
        Self {
            index,
            _phantom: PhantomData,
        }
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn map<T: ShaderType>(self) -> ShaderHandleTyped<T> {
        ShaderHandleTyped {
            index: self.index,
            _phantom: PhantomData,
        }
    }
}

impl<S: ShaderType> FromGuard for ShaderHandleTyped<S> {
    type Inner = u32;

    fn into_inner(self) -> Self::Inner {
        self.index
    }

    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            index: inner,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShaderHandle {
    shader: TypeGuard<u32>
}

impl<S: ShaderType> From<ShaderHandleTyped<S>> for ShaderHandle {
    fn from(handle: ShaderHandleTyped<S>) -> Self {
        Self {
            shader: handle.into_guard(),
        }
    }
}

impl<S: ShaderType> TryFrom<ShaderHandle> for ShaderHandleTyped<S> {
    type Error = TypeGuardError;

    fn try_from(handle: ShaderHandle) -> Result<Self, Self::Error> {
        ShaderHandleTyped::try_from_guard(handle.shader).map_err(|(_, err)| err)
    }
}
