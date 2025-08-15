use std::{
    array::TryFromSliceError,
    error::Error,
    fmt::{Display, Formatter},
    path::PathBuf,
};

use gltf::mesh::Mode;

use crate::{
    model::{Model, PbrMaps, VertexAttribute},
    shader::ShaderHandle,
};

#[derive(Debug)]
pub enum GraphicsError {
    MissingVertexIndices,
    MissingMeshData(PathBuf),
    MissingMaterialData(PathBuf),
    MissingVertexAttribute(VertexAttribute),
    InvalidDrawCall { shader: ShaderHandle, model: Model },
    UnsupportedSemantics(gltf::Semantic),
    UnsupportedPrimitive(Mode),
    Base64DecodeError(base64::DecodeError),
    GlTFError(gltf::Error),
    InvalidURI(String),
    External(String),
    SliceConversionError(TryFromSliceError),
    MissingPbrTexture(PbrMaps),
}

impl Error for GraphicsError {}

impl Display for GraphicsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphicsError::InvalidDrawCall { shader, model } => {
                write!(
                    f,
                    "Invalid draw call: shader: {:?}, model: {:?}",
                    shader, model
                )
            }
            GraphicsError::External(error) => write!(f, "External error: {}", error),
            GraphicsError::UnsupportedSemantics(semantic) => {
                write!(f, "Unsupported glTF semantics: {:?}", semantic)
            }
            GraphicsError::Base64DecodeError(error) => write!(f, "Base64 decode error: {}", error),
            GraphicsError::InvalidURI(uri) => write!(f, "Invalid URI: {}", uri),
            GraphicsError::UnsupportedPrimitive(mode) => {
                write!(f, "Unsupported primitive mode: {:?}", mode)
            }
            GraphicsError::MissingVertexAttribute(attr) => {
                write!(f, "Missing vertex attribute: {:?}", attr)
            }
            GraphicsError::MissingVertexIndices => write!(f, "Missing vertex indices in model"),
            GraphicsError::SliceConversionError(error) => {
                write!(f, "Slice conversion error: {}", error)
            }
            GraphicsError::MissingMeshData(path) => {
                write!(f, "Missing mesh data at path: {}", path.display())
            }
            GraphicsError::MissingMaterialData(path) => {
                write!(f, "Missing material data at path: {}", path.display())
            }
            GraphicsError::GlTFError(error) => write!(f, "glTF error: {}", error),
            GraphicsError::MissingPbrTexture(map) => write!(f, "Missing PBR texture: {:?}", map),
        }
    }
}

pub type GraphicsResult<T> = Result<T, GraphicsError>;

impl From<base64::DecodeError> for GraphicsError {
    fn from(error: base64::DecodeError) -> Self {
        GraphicsError::Base64DecodeError(error)
    }
}

impl From<TryFromSliceError> for GraphicsError {
    fn from(error: TryFromSliceError) -> Self {
        GraphicsError::SliceConversionError(error)
    }
}

impl From<gltf::Error> for GraphicsError {
    fn from(error: gltf::Error) -> Self {
        GraphicsError::GlTFError(error)
    }
}
