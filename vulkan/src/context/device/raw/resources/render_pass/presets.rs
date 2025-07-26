use ash::vk;

use crate::context::device::raw::resources::framebuffer::{
    presets::AttachmentsGBuffer, AttachmentList, AttachmentReference, AttachmentReferenceBuilder,
    AttachmentTarget, AttachmentTransition, AttachmentTransitionBuilder, References, Transitions,
};
use type_kit::Nil;

use super::{Cons, RenderPassBuilder, Subpass, TransitionList, TypedNil};

pub struct EmptyRenderPassTransitions {}

impl TransitionList<Nil> for EmptyRenderPassTransitions {
    fn transitions() -> Transitions<Nil> {
        unreachable!()
    }
}

pub struct EmptySubpass {}

impl Subpass<Nil> for EmptySubpass {
    fn references() -> References<Nil> {
        unreachable!()
    }
}

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
