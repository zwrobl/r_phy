use std::marker::PhantomData;

use crate::resources::{
    framebuffer::AttachmentList,
    layout::{Layout, PipelineLayout, PushConstant},
    pipeline::{get_pipeline_states_info, ModuleLoader, PipelineBindData, PushConstantDataRef},
    render_pass::RenderPass,
    render_pass::{RenderPassConfig, Subpass},
    Resource,
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

use std::{any::type_name, convert::Infallible};

use ash::vk;
use bytemuck::AnyBitPattern;
use type_kit::{Create, Destroy, DestroyResult, FromGuard, GuardVec};

use crate::{error::ResourceError, Context};

#[derive(Debug)]
pub struct GraphicsPipeline<T: GraphicsPipelineConfig> {
    handle: vk::Pipeline,
    layout: vk::PipelineLayout,
    _phantom: PhantomData<T>,
}

#[derive(Debug, Clone, Copy)]
pub struct GraphicsPipelineRaw {
    handle: vk::Pipeline,
    layout: vk::PipelineLayout,
}

impl<T: GraphicsPipelineConfig> Resource for GraphicsPipeline<T> {
    type RawType = GraphicsPipelineRaw;
    type RawCollection = GuardVec<Self::RawType>;
}

impl<T: GraphicsPipelineConfig> FromGuard for GraphicsPipeline<T> {
    type Inner = GraphicsPipelineRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner {
            handle: self.handle,
            layout: self.layout,
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            handle: inner.handle,
            layout: inner.layout,
            _phantom: PhantomData,
        }
    }
}

impl<T: GraphicsPipelineConfig> Create for GraphicsPipeline<T> {
    type Config<'a> = &'a dyn ModuleLoader;
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let modules = config;
        let layout = context.get_or_create_unique_resource::<PipelineLayout<T::Layout>, _>()?;
        let extent = context
            .physical_device
            .surface_properties
            .get_current_extent();
        let layout = layout.into();
        let render_pass =
            context.get_or_create_unique_resource::<RenderPass<T::RenderPass>, _>()?;
        let states = get_pipeline_states_info::<T::Attachments, T::Subpass, T::PipelineStates>(
            &context.physical_device,
            extent,
        );
        let modules = modules.load(context)?;
        let stages = modules.get_stages_info();
        let subpass = T::RenderPass::try_get_subpass_index::<T::Subpass>().unwrap_or_else(|| {
            panic!(
                "Subpass {} not present in RenderPass {}!",
                type_name::<T::Subpass>(),
                type_name::<T::RenderPass>(),
            )
        }) as u32;
        let create_infos = [vk::GraphicsPipelineCreateInfo {
            subpass,
            layout,
            render_pass: render_pass.handle,
            p_vertex_input_state: &states.vertex_input.create_info,
            p_input_assembly_state: &states.input_assembly,
            p_viewport_state: &states.viewport.create_info,
            p_rasterization_state: &states.rasterization,
            p_depth_stencil_state: &states.depth_stencil,
            p_color_blend_state: &states.color_blend.create_info,
            p_multisample_state: &states.multisample,
            stage_count: stages.stages.len() as u32,
            p_stages: stages.stages.as_ptr(),
            ..Default::default()
        }];
        let &handle = unsafe {
            context
                .create_graphics_pipelines(vk::PipelineCache::null(), &create_infos, None)
                .map_err(|(_, err)| err)?
                .first()
                .unwrap()
        };
        Ok(GraphicsPipeline {
            handle,
            layout,
            _phantom: PhantomData,
        })
    }
}

impl<T: GraphicsPipelineConfig> Destroy for GraphicsPipeline<T> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_pipeline(self.handle, None);
        }
        Ok(())
    }
}

impl Destroy for GraphicsPipelineRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_pipeline(self.handle, None);
        }
        Ok(())
    }
}

impl<C: GraphicsPipelineConfig> GraphicsPipeline<C> {
    #[inline]
    pub fn get_binding_data(&self) -> PipelineBindData {
        PipelineBindData {
            bind_point: vk::PipelineBindPoint::GRAPHICS,
            pipeline: self.handle,
        }
    }
}

impl<C: GraphicsPipelineConfig> From<&mut GraphicsPipeline<C>> for vk::Pipeline {
    fn from(pipeline: &mut GraphicsPipeline<C>) -> Self {
        pipeline.handle
    }
}

impl<C: GraphicsPipelineConfig> GraphicsPipeline<C> {
    pub fn layout(&self) -> PipelineLayout<C::Layout> {
        unsafe { PipelineLayout::wrap(self.layout) }
    }

    pub fn get_push_range<'a, P: PushConstant + AnyBitPattern>(
        &self,
        push_constant_data: &'a P,
    ) -> PushConstantDataRef<'a, P> {
        PushConstantDataRef {
            range: C::Layout::ranges().try_get_range::<P>().unwrap_or_else(|| {
                panic!(
                    "PushConstant {} not present in layout PushConstantRanges {}!",
                    type_name::<P>(),
                    type_name::<<C::Layout as Layout>::PushConstants>(),
                )
            }),
            layout: self.layout,
            data: push_constant_data,
        }
    }
}
