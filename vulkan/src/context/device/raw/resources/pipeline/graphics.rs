mod list;
mod pack;
mod presets;

pub use list::*;
pub use pack::*;
pub use presets::*;

use std::marker::PhantomData;

use crate::context::device::{
    raw::resources::framebuffer::AttachmentList,
    raw::resources::{
        layout::Layout,
        render_pass::{RenderPassConfig, Subpass},
    },
};

use super::PipelineStates;

pub trait GraphicsPipelineConfig: 'static {
    type Attachments: AttachmentList;
    type Layout: Layout;
    type PipelineStates: PipelineStates;
    type RenderPass: RenderPassConfig<Attachments = Self::Attachments>;
    type Subpass: Subpass<Self::Attachments>;
}

pub struct GraphicsPipelineBuilder<
    L: Layout,
    P: PipelineStates,
    R: RenderPassConfig,
    S: Subpass<R::Attachments>,
> {
    _phantom: PhantomData<(L, P, R, S)>,
}

impl<L: Layout, P: PipelineStates, R: RenderPassConfig, S: Subpass<R::Attachments>>
    GraphicsPipelineConfig for GraphicsPipelineBuilder<L, P, R, S>
{
    type Attachments = R::Attachments;
    type Layout = L;
    type PipelineStates = P;
    type RenderPass = R;
    type Subpass = S;
}
