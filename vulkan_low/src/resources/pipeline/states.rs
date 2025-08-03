mod presets;

use std::marker::PhantomData;

use ash::vk::{self, Extent2D};
pub use presets::*;

use crate::{
    device::{AttachmentProperties, PhysicalDevice, PhysicalDeviceProperties},
    resources::{
        framebuffer::{AttachmentList, AttachmentReferences, AttachmentUsage, References},
        render_pass::Subpass,
    },
};
use graphics::model::{Vertex, VertexNone};
use type_kit::{Cons, Nil};

pub struct VertexInputInfo {
    _bindings: Vec<vk::VertexInputBindingDescription>,
    _attributes: Vec<vk::VertexInputAttributeDescription>,
    pub create_info: vk::PipelineVertexInputStateCreateInfo,
}

pub trait VertexInput: 'static {
    fn get_state() -> VertexInputInfo {
        let bindings = Self::get_binding_descriptions();
        let attributes = Self::get_attribute_descriptions();
        let create_info = vk::PipelineVertexInputStateCreateInfo {
            vertex_binding_description_count: bindings.len() as u32,
            p_vertex_binding_descriptions: bindings.as_ptr(),
            vertex_attribute_description_count: attributes.len() as u32,
            p_vertex_attribute_descriptions: attributes.as_ptr(),
            ..Default::default()
        };
        VertexInputInfo {
            _bindings: bindings,
            _attributes: attributes,
            create_info,
        }
    }

    fn get_binding_descriptions() -> Vec<vk::VertexInputBindingDescription>;

    fn get_attribute_descriptions() -> Vec<vk::VertexInputAttributeDescription>;
}

pub trait VertexBinding: 'static {
    fn get_binding_description(binding: u32) -> vk::VertexInputBindingDescription;

    fn get_attribute_descriptions(binding: u32) -> Vec<vk::VertexInputAttributeDescription>;
}

fn infer_vertex_format(size: usize) -> vk::Format {
    match size {
        4 => vk::Format::R32_SFLOAT,
        8 => vk::Format::R32G32_SFLOAT,
        12 => vk::Format::R32G32B32_SFLOAT,
        16 => vk::Format::R32G32B32A32_SFLOAT,
        _ => panic!("Unsupported vertex component size"),
    }
}

impl<V: Vertex> VertexBinding for V {
    fn get_binding_description(binding: u32) -> vk::VertexInputBindingDescription {
        let last = V::components().last().unwrap();
        vk::VertexInputBindingDescription {
            binding,
            stride: (last.offset + last.size) as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }
    }

    fn get_attribute_descriptions(binding: u32) -> Vec<vk::VertexInputAttributeDescription> {
        V::components()
            .iter()
            .zip(0u32..)
            .map(
                |(component, location)| vk::VertexInputAttributeDescription {
                    binding,
                    location,
                    format: infer_vertex_format(component.size),
                    offset: component.offset as u32,
                },
            )
            .collect()
    }
}

pub trait VertexBindingList: 'static {
    type Item: VertexBinding;
    type Next: VertexBindingList;

    fn exhausted() -> bool;
    fn len() -> usize;
}

impl VertexBindingList for Nil {
    type Item = VertexNone;
    type Next = Self;

    fn exhausted() -> bool {
        true
    }

    fn len() -> usize {
        0
    }
}

impl<B: VertexBinding, N: VertexBindingList> VertexBindingList for Cons<B, N> {
    type Item = B;
    type Next = N;

    fn exhausted() -> bool {
        false
    }

    fn len() -> usize {
        Self::Next::len() + 1
    }
}

pub struct VertexBindingBuilder<L: VertexBindingList> {
    _phantom: PhantomData<L>,
}

impl<L: VertexBindingList> VertexBindingBuilder<L> {
    fn next_binding_description<'a, N: VertexBindingList>(
        binding: u32,
        mut iter: impl Iterator<Item = &'a mut vk::VertexInputBindingDescription>,
    ) {
        if !N::exhausted() {
            if let Some(entry) = iter.next() {
                *entry = N::Item::get_binding_description(binding);
                Self::next_binding_description::<N::Next>(binding + 1, iter);
            }
        }
    }

    pub fn get_binding_descriptions() -> Vec<vk::VertexInputBindingDescription> {
        let mut bindings = vec![vk::VertexInputBindingDescription::default(); L::len()];
        Self::next_binding_description::<L>(0, bindings.iter_mut());
        bindings
    }

    fn next_attribute_descriptions<'a, N: VertexBindingList>(
        binding: u32,
        mut iter: impl Iterator<Item = &'a mut Vec<vk::VertexInputAttributeDescription>>,
    ) {
        if !N::exhausted() {
            if let Some(entry) = iter.next() {
                *entry = N::Item::get_attribute_descriptions(binding);
                Self::next_attribute_descriptions::<N::Next>(binding + 1, iter)
            }
        }
    }

    pub fn get_attribute_descriptions() -> Vec<vk::VertexInputAttributeDescription> {
        let mut attributes = vec![vec![]; L::len()];
        Self::next_attribute_descriptions::<L>(0, attributes.iter_mut());
        attributes.into_iter().flatten().collect()
    }
}

impl<L: VertexBindingList> VertexInput for VertexBindingBuilder<L> {
    fn get_binding_descriptions() -> Vec<vk::VertexInputBindingDescription> {
        Self::get_binding_descriptions()
    }

    fn get_attribute_descriptions() -> Vec<vk::VertexInputAttributeDescription> {
        Self::get_attribute_descriptions()
    }
}

pub trait VertexAssembly: 'static {
    fn get_input_assembly() -> vk::PipelineInputAssemblyStateCreateInfo;
}

pub trait DepthStencil: 'static {
    fn get_state() -> vk::PipelineDepthStencilStateCreateInfo;
}

pub trait Rasterization: 'static {
    fn get_state() -> vk::PipelineRasterizationStateCreateInfo;
}

pub struct ViewportInfo {
    _viewports: Vec<vk::Viewport>,
    _scissors: Vec<vk::Rect2D>,
    pub create_info: vk::PipelineViewportStateCreateInfo,
}

pub trait Viewport: 'static {
    fn get_state(image_extent: vk::Extent2D) -> ViewportInfo;
}

pub struct ColorBlendInfo {
    _attachments: Vec<vk::PipelineColorBlendAttachmentState>,
    pub create_info: vk::PipelineColorBlendStateCreateInfo,
}

pub trait Blend: 'static {
    const BLEND: vk::PipelineColorBlendAttachmentState;
}

pub trait ColorBlend: 'static {
    fn get_state<A: AttachmentList>(references: &References<A>) -> ColorBlendInfo;
}

pub struct ColorBlendBuilder<B: Blend> {
    _phantom: PhantomData<B>,
}

impl<B: Blend> ColorBlend for ColorBlendBuilder<B> {
    fn get_state<A: AttachmentList>(references: &References<A>) -> ColorBlendInfo {
        let attachments = references
            .get_references()
            .into_iter()
            .filter_map(|reference| {
                if let Some(reference) = reference {
                    if reference.try_get_usage() == Some(AttachmentUsage::Color) {
                        return Some(B::BLEND);
                    }
                }
                None
            })
            .collect::<Vec<_>>();
        let create_info = vk::PipelineColorBlendStateCreateInfo {
            attachment_count: attachments.len() as u32,
            p_attachments: attachments.as_ptr(),
            ..Default::default()
        };
        ColorBlendInfo {
            _attachments: attachments,
            create_info,
        }
    }
}

pub trait Multisample: 'static {
    fn get_state(
        device: &PhysicalDeviceProperties,
        attachments: &AttachmentProperties,
    ) -> vk::PipelineMultisampleStateCreateInfo;
}

pub trait PipelineStates: 'static {
    type VertexInput: VertexInput;
    type VertexAssembly: VertexAssembly;
    type DepthStencil: DepthStencil;
    type Rasterization: Rasterization;
    type Viewport: Viewport;
    type ColorBlend: ColorBlend;
    type Multisample: Multisample;
}

#[derive(Debug, Clone, Copy)]
pub struct PipelineStatesBuilder<
    I: VertexInput,
    A: VertexAssembly,
    D: DepthStencil,
    R: Rasterization,
    V: Viewport,
    C: ColorBlend,
    M: Multisample,
> {
    _phantom: PhantomData<(I, A, D, R, V, C, M)>,
}

impl<
        I: VertexInput,
        A: VertexAssembly,
        D: DepthStencil,
        R: Rasterization,
        V: Viewport,
        C: ColorBlend,
        M: Multisample,
    > Default for PipelineStatesBuilder<I, A, D, R, V, C, M>
{
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

#[allow(dead_code)]
impl<
        I: VertexInput,
        A: VertexAssembly,
        D: DepthStencil,
        R: Rasterization,
        V: Viewport,
        C: ColorBlend,
        M: Multisample,
    > PipelineStatesBuilder<I, A, D, R, V, C, M>
{
    pub fn builder() -> Self {
        Self::default()
    }

    pub fn with_vertex_input<N: VertexInput>(self) -> PipelineStatesBuilder<N, A, D, R, V, C, M> {
        PipelineStatesBuilder {
            _phantom: PhantomData,
        }
    }

    pub fn with_assembly<N: VertexAssembly>(self) -> PipelineStatesBuilder<I, N, D, R, V, C, M> {
        PipelineStatesBuilder {
            _phantom: PhantomData,
        }
    }

    pub fn with_depth_stencil<N: DepthStencil>(self) -> PipelineStatesBuilder<I, A, N, R, V, C, M> {
        PipelineStatesBuilder {
            _phantom: PhantomData,
        }
    }

    pub fn with_rasterization<N: Rasterization>(
        self,
    ) -> PipelineStatesBuilder<I, A, D, N, V, C, M> {
        PipelineStatesBuilder {
            _phantom: PhantomData,
        }
    }

    pub fn with_viewport<N: Viewport>(self) -> PipelineStatesBuilder<I, A, D, R, N, C, M> {
        PipelineStatesBuilder {
            _phantom: PhantomData,
        }
    }

    pub fn with_color_blend<N: ColorBlend>(self) -> PipelineStatesBuilder<I, A, D, R, V, N, M> {
        PipelineStatesBuilder {
            _phantom: PhantomData,
        }
    }

    pub fn with_multisample<N: Multisample>(self) -> PipelineStatesBuilder<I, A, D, R, V, C, N> {
        PipelineStatesBuilder {
            _phantom: PhantomData,
        }
    }
}

impl<
        I: VertexInput,
        A: VertexAssembly,
        D: DepthStencil,
        R: Rasterization,
        V: Viewport,
        C: ColorBlend,
        M: Multisample,
    > PipelineStates for PipelineStatesBuilder<I, A, D, R, V, C, M>
{
    type VertexInput = I;
    type VertexAssembly = A;
    type DepthStencil = D;
    type Rasterization = R;
    type Viewport = V;
    type ColorBlend = C;
    type Multisample = M;
}

pub struct PipelineStatesInfo<S: PipelineStates> {
    pub vertex_input: VertexInputInfo,
    pub input_assembly: vk::PipelineInputAssemblyStateCreateInfo,
    pub viewport: ViewportInfo,
    pub rasterization: vk::PipelineRasterizationStateCreateInfo,
    pub depth_stencil: vk::PipelineDepthStencilStateCreateInfo,
    pub color_blend: ColorBlendInfo,
    pub multisample: vk::PipelineMultisampleStateCreateInfo,
    _phantom: PhantomData<S>,
}

pub(super) fn get_pipeline_states_info<A: AttachmentList, P: Subpass<A>, S: PipelineStates>(
    physical_device: &PhysicalDevice,
    extent: Extent2D,
) -> PipelineStatesInfo<S> {
    PipelineStatesInfo {
        vertex_input: S::VertexInput::get_state(),
        input_assembly: S::VertexAssembly::get_input_assembly(),
        viewport: S::Viewport::get_state(extent),
        rasterization: S::Rasterization::get_state(),
        depth_stencil: S::DepthStencil::get_state(),
        color_blend: S::ColorBlend::get_state::<A>(&P::references()),
        multisample: S::Multisample::get_state(
            &physical_device.properties,
            &physical_device.attachment_properties,
        ),
        _phantom: PhantomData,
    }
}
