use graphics::shader::Shader;

use crate::frame::Frame;

pub mod deferred;

pub type RendererShader<R, N, T> = <R as Frame>::Shader<Shader<N, T>>;
