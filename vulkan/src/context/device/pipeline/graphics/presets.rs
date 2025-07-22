use graphics::model::CommonVertex;

use crate::context::device::{
    pipeline::{StatesDepthTestEnabled, StatesDepthWriteDisabled, StatesSkybox},
    raw::unique::{
        layout::presets::{PipelineLayoutGBuffer, PipelineLayoutNoMaterial, PipelineLayoutSkybox},
        render_pass::presets::{
            DeferedRenderPass, GBufferDepthPrepas, GBufferShadingPass, GBufferSkyboxPass,
        },
    },
};

use super::GraphicsPipelineBuilder;

// pub type EmptyPipeline = GraphicsPipelineBuilder<
//     PipelineLayoutNoMaterial,
//     StatesDepthWriteDisabled<VertexNone>,
//     EmptyRenderPass,
//     EmptySubpass,
// >;

pub type GBufferSkyboxPipeline<At> = GraphicsPipelineBuilder<
    PipelineLayoutSkybox,
    StatesSkybox,
    DeferedRenderPass<At>,
    GBufferSkyboxPass<At>,
>;

pub type GBufferDepthPrepasPipeline<A> = GraphicsPipelineBuilder<
    PipelineLayoutNoMaterial,
    StatesDepthTestEnabled<CommonVertex>,
    DeferedRenderPass<A>,
    GBufferDepthPrepas<A>,
>;

pub type GBufferShadingPassPipeline<A> = GraphicsPipelineBuilder<
    PipelineLayoutGBuffer,
    StatesDepthWriteDisabled<CommonVertex>,
    DeferedRenderPass<A>,
    GBufferShadingPass<A>,
>;
