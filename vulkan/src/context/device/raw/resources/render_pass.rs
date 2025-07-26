pub mod presets;

use std::{any::TypeId, collections::HashMap, convert::Infallible, marker::PhantomData};

use ash::vk;

use crate::context::{
    device::{
        framebuffer::{
            AttachmentFormatInfo, AttachmentList, AttachmentListFormats, AttachmentReference,
            AttachmentReferences, AttachmentTarget, AttachmentTransistions, AttachmentTransition,
            IndexedAttachmentReference, References, Transitions,
        },
        raw::resources::TypeUniqueResource,
        AttachmentProperties,
    },
    error::ResourceError,
    Context,
};
use type_kit::{Cons, Create, Destroy, FromGuard, TypedNil};

fn get_descriptions(
    formats: Vec<AttachmentFormatInfo>,
    transitions: Vec<AttachmentTransition>,
) -> Vec<vk::AttachmentDescription> {
    formats
        .into_iter()
        .zip(transitions)
        .map(|(format, transition)| vk::AttachmentDescription {
            format: format.format,
            samples: format.samples,
            load_op: transition.load_op,
            store_op: transition.store_op,
            initial_layout: transition.initial_layout,
            final_layout: transition.final_layout,
            stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
            stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
            ..Default::default()
        })
        .collect()
}

pub trait TransitionList<A: AttachmentList>: 'static {
    fn transitions() -> Transitions<A>;

    fn get_descriptions(properties: &AttachmentProperties) -> Vec<vk::AttachmentDescription> {
        get_descriptions(
            <A as AttachmentListFormats>::values(properties)
                .into_iter()
                .rev()
                .collect(),
            Self::transitions().get(),
        )
        .into_iter()
        .collect()
    }
}

struct AttachmentUsageFlags {
    stage: vk::PipelineStageFlags,
    access: vk::AccessFlags,
}

impl AttachmentReference {
    fn get_flags(&self) -> AttachmentUsageFlags {
        if self.usage.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT) {
            AttachmentUsageFlags {
                stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            }
        } else if self
            .usage
            .contains(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
        {
            AttachmentUsageFlags {
                stage: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
            }
        } else if self.usage.contains(vk::ImageUsageFlags::INPUT_ATTACHMENT) {
            AttachmentUsageFlags {
                stage: vk::PipelineStageFlags::FRAGMENT_SHADER,
                access: vk::AccessFlags::INPUT_ATTACHMENT_READ,
            }
        } else {
            panic!("Unsupported image usage!");
        }
    }
}

pub struct SubpassDescription {
    _references: Vec<vk::AttachmentReference>,
    description: vk::SubpassDescription,
}

impl SubpassDescription {
    pub fn get_references(
        references: Vec<Option<IndexedAttachmentReference>>,
    ) -> Vec<(AttachmentTarget, vk::AttachmentReference)> {
        references
            .into_iter()
            .filter_map(|reference| {
                if let Some(IndexedAttachmentReference { reference, index }) = reference {
                    Some((
                        reference.target,
                        vk::AttachmentReference {
                            attachment: index,
                            layout: reference.layout,
                        },
                    ))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get<R: AttachmentReferences>(references: &R) -> Self {
        let mut references = Self::get_references(references.get_references());
        references.sort_by_key(|(target, _)| *target as usize);
        let preserve = references
            .iter()
            .filter(|(target, _)| *target == AttachmentTarget::Preserve)
            .map(|(_, reference)| reference.attachment)
            .collect::<Vec<_>>();

        let num_color = references
            .iter()
            .filter(|(target, _)| *target == AttachmentTarget::Color)
            .count();

        let num_depth_stencil = references
            .iter()
            .filter(|(target, _)| *target == AttachmentTarget::DepthStencil)
            .count();

        let num_resolve = references
            .iter()
            .filter(|(target, _)| *target == AttachmentTarget::Resolve)
            .count();

        let num_input = references
            .iter()
            .filter(|(target, _)| *target == AttachmentTarget::Input)
            .count();

        let references = references
            .into_iter()
            .map(|(_, reference)| reference)
            .collect::<Vec<_>>();

        let description = vk::SubpassDescription {
            pipeline_bind_point: vk::PipelineBindPoint::GRAPHICS,
            color_attachment_count: num_color as u32,
            p_color_attachments: if num_color != 0 {
                &references[0]
            } else {
                std::ptr::null()
            },
            p_resolve_attachments: if num_resolve != 0 {
                &references[num_color + num_depth_stencil]
            } else {
                std::ptr::null()
            },
            p_depth_stencil_attachment: if num_depth_stencil != 0 {
                &references[num_color]
            } else {
                std::ptr::null()
            },
            input_attachment_count: num_input as u32,
            p_input_attachments: if num_input != 0 {
                &references[num_color + num_depth_stencil + num_resolve]
            } else {
                std::ptr::null()
            },
            preserve_attachment_count: preserve.len() as u32,
            p_preserve_attachments: if !preserve.is_empty() {
                preserve.as_ptr()
            } else {
                std::ptr::null()
            },
            ..Default::default()
        };
        Self {
            _references: references,
            description,
        }
    }
}

struct SubpassInfo {
    description: SubpassDescription,
    references: Vec<Option<IndexedAttachmentReference>>,
}

fn get_subpass_info<A: AttachmentList, S: Subpass<A>>() -> SubpassInfo {
    let references = S::references();
    let description = SubpassDescription::get(&references);
    let references = references.get_references().into_iter().collect();
    SubpassInfo {
        description,
        references,
    }
}

pub trait Subpass<A: AttachmentList>: 'static {
    fn references() -> References<A>;
}

pub trait SubpassList: 'static {
    const LEN: usize;
    type Attachments: AttachmentList;
    type Item: Subpass<Self::Attachments>;
    type Next: SubpassList<Attachments = Self::Attachments>;

    fn try_get_subpass_index<N: Subpass<Self::Attachments>>() -> Option<usize> {
        if Self::LEN > 0 {
            if TypeId::of::<Self::Item>() == TypeId::of::<N>() {
                Some(Self::LEN - 1)
            } else {
                Self::Next::try_get_subpass_index::<N>()
            }
        } else {
            None
        }
    }

    fn get_description() -> SubpassDescription;

    fn get_references() -> Vec<Option<IndexedAttachmentReference>>;
}

impl<A: AttachmentList> Subpass<A> for TypedNil<A> {
    fn references() -> References<A> {
        unreachable!()
    }
}

impl<A: AttachmentList> SubpassList for TypedNil<A> {
    const LEN: usize = 0;
    type Attachments = A;
    type Item = Self;
    type Next = Self;

    fn get_description() -> SubpassDescription {
        unreachable!()
    }

    fn get_references() -> Vec<Option<IndexedAttachmentReference>> {
        unreachable!()
    }
}

impl<L: SubpassList, S: Subpass<L::Attachments>> SubpassList for Cons<S, L> {
    const LEN: usize = Self::Next::LEN + 1;
    type Attachments = L::Attachments;
    type Item = S;
    type Next = L;

    fn get_description() -> SubpassDescription {
        let SubpassInfo { description, .. } = get_subpass_info::<_, S>();
        description
    }

    fn get_references() -> Vec<Option<IndexedAttachmentReference>> {
        let SubpassInfo { references, .. } = get_subpass_info::<_, S>();
        references
    }
}

#[derive(Debug, Clone, Copy)]
struct AttachmenState {
    subpass: usize,
    reference: AttachmentReference,
}

pub struct SubpassDependencyBuilder<L: SubpassList> {
    _phantom: PhantomData<L>,
}

impl<L: SubpassList> SubpassDependencyBuilder<L> {
    fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }

    fn next_reference<N: SubpassList>(vec: &mut Vec<Vec<Option<IndexedAttachmentReference>>>) {
        if N::LEN > 0 {
            vec.push(N::get_references());
            Self::next_reference::<N::Next>(vec)
        }
    }

    fn get_references(&self) -> Vec<Vec<Option<IndexedAttachmentReference>>> {
        let mut references = Vec::with_capacity(L::LEN);
        Self::next_reference::<L>(&mut references);
        references
    }

    fn get_dependencies(
        state: &mut [Option<AttachmenState>],
        next: &[Option<IndexedAttachmentReference>],
        dst_subpass: usize,
    ) -> Vec<vk::SubpassDependency> {
        let mut dependencies = HashMap::<usize, vk::SubpassDependency>::new();
        for (current, next) in state.iter_mut().zip(next.iter()) {
            if let Some(next) = next {
                let (src_subpass, src_flags) = if let Some(current) = current {
                    (current.subpass, current.reference.get_flags())
                } else {
                    (
                        vk::SUBPASS_EXTERNAL as usize,
                        AttachmentUsageFlags {
                            stage: vk::PipelineStageFlags::TOP_OF_PIPE,
                            access: vk::AccessFlags::empty(),
                        },
                    )
                };
                let dst_flags = next.reference.get_flags();
                current.replace(AttachmenState {
                    subpass: dst_subpass,
                    reference: next.reference,
                });
                dependencies
                    .entry(src_subpass)
                    .and_modify(|dependency| {
                        dependency.src_stage_mask |= src_flags.stage;
                        dependency.dst_stage_mask |= dst_flags.stage;
                        dependency.src_access_mask |= src_flags.access;
                        dependency.dst_access_mask |= dst_flags.access;
                    })
                    .or_insert(vk::SubpassDependency {
                        src_subpass: src_subpass as u32,
                        dst_subpass: dst_subpass as u32,
                        src_stage_mask: src_flags.stage,
                        dst_stage_mask: dst_flags.stage,
                        src_access_mask: src_flags.access,
                        dst_access_mask: dst_flags.access,
                        dependency_flags: vk::DependencyFlags::empty(),
                    });
            }
        }
        dependencies.into_values().collect()
    }

    fn build(&self) -> Vec<vk::SubpassDependency> {
        let references = self.get_references();
        let mut state = vec![None; references.first().unwrap().len()];
        let mut dependeicies = Vec::new();
        for (subpass_index, attachments) in references.iter().enumerate() {
            dependeicies.extend(Self::get_dependencies(
                &mut state,
                attachments,
                subpass_index,
            ))
        }
        let mut external_dependeicies = vec![];
        for current in state.into_iter().flatten() {
            let src_flags = current.reference.get_flags();
            let src_subpass = current.subpass;
            let dst_flags = AttachmentUsageFlags {
                stage: vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                access: vk::AccessFlags::MEMORY_READ,
            };
            let dst_subpass = vk::SUBPASS_EXTERNAL as usize;
            external_dependeicies.push(vk::SubpassDependency {
                src_subpass: src_subpass as u32,
                dst_subpass: dst_subpass as u32,
                src_stage_mask: src_flags.stage,
                dst_stage_mask: dst_flags.stage,
                src_access_mask: src_flags.access,
                dst_access_mask: dst_flags.access,
                dependency_flags: vk::DependencyFlags::empty(),
            })
        }
        dependeicies.extend(external_dependeicies);
        dependeicies
    }
}

pub struct RenderPassBuilder<S: SubpassList, T: TransitionList<S::Attachments>> {
    _phantom: PhantomData<(S, T)>,
}

fn write_descriptions<N: SubpassList>(mut vec: Vec<SubpassDescription>) -> Vec<SubpassDescription> {
    if N::LEN > 0 {
        vec.push(N::get_description());
        write_descriptions::<N::Next>(vec)
    } else {
        vec
    }
}

impl<S: SubpassList, T: TransitionList<S::Attachments>> RenderPassBuilder<S, T> {
    fn get_attachment_descriptions(
        properties: &AttachmentProperties,
    ) -> Vec<vk::AttachmentDescription> {
        T::get_descriptions(properties)
    }

    fn get_subpass_descriptions() -> Vec<SubpassDescription> {
        let mut descriptions = write_descriptions::<S>(Vec::with_capacity(S::LEN));
        descriptions.reverse();
        descriptions
    }

    fn get_subpass_dependencies() -> Vec<vk::SubpassDependency> {
        SubpassDependencyBuilder::<S>::new().build()
    }
}

pub trait RenderPassConfig: 'static {
    type Attachments: AttachmentList;
    type Subpasses: SubpassList<Attachments = Self::Attachments>;
    type Transitions: TransitionList<Self::Attachments>;

    fn try_get_subpass_index<N: Subpass<Self::Attachments>>() -> Option<usize> {
        Self::Subpasses::try_get_subpass_index::<N>()
    }

    fn get_attachment_descriptions(
        properties: &AttachmentProperties,
    ) -> Vec<vk::AttachmentDescription>;

    fn get_subpass_descriptions() -> Vec<SubpassDescription>;

    fn get_subpass_dependencies() -> Vec<vk::SubpassDependency>;
}

impl<S: SubpassList, T: TransitionList<S::Attachments>> RenderPassConfig
    for RenderPassBuilder<S, T>
{
    type Attachments = S::Attachments;
    type Transitions = T;
    type Subpasses = S;

    fn get_attachment_descriptions(
        properties: &AttachmentProperties,
    ) -> Vec<vk::AttachmentDescription> {
        Self::get_attachment_descriptions(properties)
    }

    fn get_subpass_descriptions() -> Vec<SubpassDescription> {
        Self::get_subpass_descriptions()
    }

    fn get_subpass_dependencies() -> Vec<vk::SubpassDependency> {
        Self::get_subpass_dependencies()
    }
}

#[derive(Debug)]
pub struct RenderPass<C: RenderPassConfig> {
    pub handle: vk::RenderPass,
    _phantom: PhantomData<C>,
}

impl<C: RenderPassConfig> Clone for RenderPass<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: RenderPassConfig> Copy for RenderPass<C> {}

#[derive(Debug, Clone, Copy)]
pub struct RenderPassRaw {
    pub handle: vk::RenderPass,
}

impl<C: RenderPassConfig> TypeUniqueResource for RenderPass<C> {
    type RawType = RenderPassRaw;
}

impl<C: RenderPassConfig> FromGuard for RenderPass<C> {
    type Inner = RenderPassRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner {
            handle: self.handle,
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            handle: inner.handle,
            _phantom: PhantomData,
        }
    }
}

impl<C: RenderPassConfig> Create for RenderPass<C> {
    type Config<'a> = ();

    type CreateError = ResourceError;

    fn create<'a, 'b>(
        _config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let attachments =
            C::get_attachment_descriptions(&context.physical_device.attachment_properties);
        let subpasses = C::get_subpass_descriptions();
        let vk_subpasses = subpasses
            .iter()
            .map(|description| description.description)
            .collect::<Vec<_>>();
        let dependencies = C::get_subpass_dependencies();
        let create_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attachments)
            .subpasses(&vk_subpasses)
            .dependencies(&dependencies);
        let handle = unsafe { context.create_render_pass(&create_info, None)? };
        Ok(Self {
            handle,
            _phantom: PhantomData,
        })
    }
}

impl<C: RenderPassConfig> Destroy for RenderPass<C> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        unsafe {
            context.destroy_render_pass(self.handle, None);
        }
        Ok(())
    }
}

impl Destroy for RenderPassRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        unsafe {
            context.destroy_render_pass(self.handle, None);
        }
        Ok(())
    }
}
