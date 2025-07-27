use ash::vk;
use graphics::{model::CommonVertex, renderer::camera::CameraMatrices};
use type_kit::{Cons, Nil, TypedNil};
use vulkan_low::device::raw::resources::{
    framebuffer::{
        presets::{ColorMultisampled, DepthStencilMultisampled, Resolve},
        AttachmentImage, AttachmentList, AttachmentReference, AttachmentReferenceBuilder,
        AttachmentTarget, AttachmentTransition, AttachmentTransitionBuilder, InputAttachment,
        References, Transitions,
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
            .push(AttachmentTransition {
                // Combined
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            })
            .push(AttachmentTransition {
                // Albedo
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            })
            .push(AttachmentTransition {
                // Normal
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            })
            .push(AttachmentTransition {
                // Position
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            })
            .push(AttachmentTransition {
                // Depth
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            })
            .push(AttachmentTransition {
                // Resolve
                load_op: vk::AttachmentLoadOp::DONT_CARE,
                store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
            })
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
            .push(Some(AttachmentReference {
                target: AttachmentTarget::DepthStencil,
                layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            }))
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
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Color,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
            }))
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Color,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
            }))
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Color,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
            }))
            .push(Some(AttachmentReference {
                target: AttachmentTarget::DepthStencil,
                layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            }))
            .push(None)
    }
}

pub struct GBufferShadingPass<A: AttachmentList> {
    _phantom: std::marker::PhantomData<A>,
}

impl Subpass<AttachmentsGBuffer> for GBufferShadingPass<AttachmentsGBuffer> {
    fn references() -> References<AttachmentsGBuffer> {
        AttachmentReferenceBuilder::new()
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Color,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
            }))
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Input,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                usage: vk::ImageUsageFlags::INPUT_ATTACHMENT,
            }))
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Input,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                usage: vk::ImageUsageFlags::INPUT_ATTACHMENT,
            }))
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Input,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                usage: vk::ImageUsageFlags::INPUT_ATTACHMENT,
            }))
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Input,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                usage: vk::ImageUsageFlags::INPUT_ATTACHMENT,
            }))
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Resolve,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
            }))
    }
}

pub struct GBufferSkyboxPass<A: AttachmentList> {
    _phantom: std::marker::PhantomData<A>,
}

impl Subpass<AttachmentsGBuffer> for GBufferSkyboxPass<AttachmentsGBuffer> {
    fn references() -> References<AttachmentsGBuffer> {
        AttachmentReferenceBuilder::new()
            .push(Some(AttachmentReference {
                target: AttachmentTarget::Color,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
            }))
            .push(None)
            .push(None)
            .push(None)
            .push(Some(AttachmentReference {
                target: AttachmentTarget::DepthStencil,
                layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            }))
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
