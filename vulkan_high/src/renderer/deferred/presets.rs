use graphics::{model::CommonVertex, renderer::camera::CameraMatrices};
use type_kit::{Cons, Nil, TypedNil};
use vulkan_low::resources::{
    framebuffer::{
        presets::{ColorMultisampled, DepthStencilMultisampled, Resolve},
        AttachmentImage, AttachmentList, AttachmentReferenceBuilder, AttachmentTarget,
        AttachmentTransition, AttachmentTransitionBuilder, AttachmentUsage, ImageLayout,
        InputAttachment, LoadOp, References, StoreOp, Transitions,
    },
    layout::{
        presets::{CameraDescriptorSet, ModelMatrix, ModelNormalMatrix, TextureDescriptorSet},
        DescriptorLayoutBuilder, PipelineLayoutBuilder,
    },
    pipeline::{
        AlphaBlend, CullBack, CullFront, DepthTestEnabled, DepthWriteDisabled,
        GraphicsPipelineBuilder, MeshVertexInput, Multisampled, PipelineStatesBuilder,
        TriangleList, ViewportDefault,
    },
    render_pass::{RenderPassBuilder, Subpass, TransitionList},
};

use crate::resources::Material;

pub type PipelineLayoutMaterial<M> = PipelineLayoutBuilder<
    Cons<<M as Material>::DescriptorLayout, Cons<CameraDescriptorSet, Nil>>,
    Cons<ModelNormalMatrix, Nil>,
>;

pub type PipelineLayoutSkybox =
    PipelineLayoutBuilder<Cons<TextureDescriptorSet, Nil>, Cons<CameraMatrices, Nil>>;

pub type PipelineLayoutNoMaterial =
    PipelineLayoutBuilder<Cons<CameraDescriptorSet, Nil>, Cons<ModelMatrix, Nil>>;

pub type PipelineLayoutGBuffer = PipelineLayoutBuilder<Cons<GBufferDescriptorSet, Nil>, Nil>;

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

pub struct DeferedRenderPassTransitions<A: AttachmentList> {
    _phantom: std::marker::PhantomData<A>,
}

impl TransitionList<AttachmentsGBuffer> for DeferedRenderPassTransitions<AttachmentsGBuffer> {
    fn transitions() -> Transitions<AttachmentsGBuffer> {
        AttachmentTransitionBuilder::new()
            .push(
                // Combined
                AttachmentTransition::new(
                    LoadOp::Clear,
                    StoreOp::DontCare,
                    ImageLayout::Undefined,
                    ImageLayout::ColorAttachment,
                ),
            )
            .push(
                // Albedo
                AttachmentTransition::new(
                    LoadOp::Clear,
                    StoreOp::DontCare,
                    ImageLayout::Undefined,
                    ImageLayout::ShaderReadOnly,
                ),
            )
            .push(
                // Normal
                AttachmentTransition::new(
                    LoadOp::Clear,
                    StoreOp::DontCare,
                    ImageLayout::Undefined,
                    ImageLayout::ShaderReadOnly,
                ),
            )
            .push(
                // Position
                AttachmentTransition::new(
                    LoadOp::Clear,
                    StoreOp::DontCare,
                    ImageLayout::Undefined,
                    ImageLayout::ShaderReadOnly,
                ),
            )
            .push(
                // Depth
                AttachmentTransition::new(
                    LoadOp::Clear,
                    StoreOp::DontCare,
                    ImageLayout::Undefined,
                    ImageLayout::ShaderReadOnly,
                ),
            )
            .push(
                // Resolve
                AttachmentTransition::new(
                    LoadOp::DontCare,
                    StoreOp::DontCare,
                    ImageLayout::Undefined,
                    ImageLayout::PresentSrc,
                ),
            )
    }
}

pub struct GBufferDepthPrepas<A: AttachmentList> {
    _phantom: std::marker::PhantomData<A>,
}

impl Subpass<AttachmentsGBuffer> for GBufferDepthPrepas<AttachmentsGBuffer> {
    fn references() -> References<AttachmentsGBuffer> {
        AttachmentReferenceBuilder::new()
            .push(None)
            .push(None)
            .push(None)
            .push(None)
            .push(Some(AttachmentTarget::Use(AttachmentUsage::DepthStencil)))
            .push(None)
    }
}

pub struct GBufferWritePass<A: AttachmentList> {
    _phantom: std::marker::PhantomData<A>,
}

impl Subpass<AttachmentsGBuffer> for GBufferWritePass<AttachmentsGBuffer> {
    fn references() -> References<AttachmentsGBuffer> {
        AttachmentReferenceBuilder::new()
            .push(None)
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Color)))
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Color)))
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Color)))
            .push(Some(AttachmentTarget::Use(AttachmentUsage::DepthStencil)))
            .push(None)
    }
}

pub struct GBufferShadingPass<A: AttachmentList> {
    _phantom: std::marker::PhantomData<A>,
}

impl Subpass<AttachmentsGBuffer> for GBufferShadingPass<AttachmentsGBuffer> {
    fn references() -> References<AttachmentsGBuffer> {
        AttachmentReferenceBuilder::new()
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Color)))
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Input)))
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Input)))
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Input)))
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Input)))
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Resolve)))
    }
}

pub struct GBufferSkyboxPass<A: AttachmentList> {
    _phantom: std::marker::PhantomData<A>,
}

impl Subpass<AttachmentsGBuffer> for GBufferSkyboxPass<AttachmentsGBuffer> {
    fn references() -> References<AttachmentsGBuffer> {
        AttachmentReferenceBuilder::new()
            .push(Some(AttachmentTarget::Use(AttachmentUsage::Color)))
            .push(None)
            .push(None)
            .push(None)
            .push(Some(AttachmentTarget::Use(AttachmentUsage::DepthStencil)))
            .push(None)
    }
}

pub type DeferedRenderPass<A> = RenderPassBuilder<
    Cons<
        GBufferShadingPass<A>,
        Cons<
            GBufferWritePass<A>,
            Cons<GBufferSkyboxPass<A>, Cons<GBufferDepthPrepas<A>, TypedNil<A>>>,
        >,
    >,
    DeferedRenderPassTransitions<A>,
>;

pub type StatesSkybox = PipelineStatesBuilder<
    MeshVertexInput<CommonVertex>,
    TriangleList,
    DepthWriteDisabled,
    CullFront,
    ViewportDefault,
    AlphaBlend,
    Multisampled,
>;

pub type StatesDepthWriteDisabled<V> = PipelineStatesBuilder<
    MeshVertexInput<V>,
    TriangleList,
    DepthWriteDisabled,
    CullBack,
    ViewportDefault,
    AlphaBlend,
    Multisampled,
>;

pub type StatesDepthTestEnabled<V> = PipelineStatesBuilder<
    MeshVertexInput<V>,
    TriangleList,
    DepthTestEnabled,
    CullBack,
    ViewportDefault,
    AlphaBlend,
    Multisampled,
>;

pub type GBufferDescriptorSet = DescriptorLayoutBuilder<
    Cons<
        // Albedo
        InputAttachment,
        Cons<
            // Position
            InputAttachment,
            Cons<
                // Normal
                InputAttachment,
                Cons<
                    // Depth
                    InputAttachment,
                    Nil,
                >,
            >,
        >,
    >,
>;

pub type AttachmentsGBuffer = Cons<
    AttachmentImage<ColorMultisampled>, // Combined
    Cons<
        AttachmentImage<ColorMultisampled>, // Albedo
        Cons<
            AttachmentImage<ColorMultisampled>, // Normal
            Cons<
                AttachmentImage<ColorMultisampled>, // Position
                Cons<
                    AttachmentImage<DepthStencilMultisampled>,
                    Cons<AttachmentImage<Resolve>, Nil>,
                >,
            >,
        >,
    >,
>;
