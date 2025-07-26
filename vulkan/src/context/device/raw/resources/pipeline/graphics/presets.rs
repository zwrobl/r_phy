use graphics::model::CommonVertex;

use crate::context::device::{
    raw::resources::pipeline::{StatesDepthTestEnabled, StatesDepthWriteDisabled, StatesSkybox},
    raw::resources::{
        layout::presets::{PipelineLayoutGBuffer, PipelineLayoutNoMaterial, PipelineLayoutSkybox},
        render_pass::presets::{
            DeferedRenderPass, GBufferDepthPrepas, GBufferShadingPass, GBufferSkyboxPass,
        },
    },
};

use super::GraphicsPipelineBuilder;

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
