use std::{
    any::{type_name, TypeId},
    convert::Infallible,
    marker::PhantomData,
};

use ash::vk;
use bytemuck::AnyBitPattern;
use type_kit::{Create, CreateResult, Destroy, DestroyResult, FromGuard, TypeGuardCollection};

use crate::{
    device::raw::{
        resources::{
            layout::{Layout, PipelineLayout, PushConstant},
            render_pass::{RenderPass, RenderPassConfig},
        },
        resources::{
            pipeline::{
                get_pipeline_states_info, ModuleLoader, PipelineBindData, PushConstantDataRef,
            },
            Resource,
        },
    },
    error::{ResourceError, VkError, VkResult},
    Context,
};

use super::GraphicsPipelineConfig;

#[derive(Debug)]
pub struct PipelinePackData {
    pipelines: Vec<vk::Pipeline>,
    layout: vk::PipelineLayout,
}

#[derive(Debug)]
pub struct PipelinePack<T: GraphicsPipelineConfig> {
    data: PipelinePackData,
    _phantom: PhantomData<T>,
}

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
    type RawCollection = TypeGuardCollection<Self::RawType>;
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

impl<C: GraphicsPipelineConfig> From<&GraphicsPipeline<C>> for PipelineBindData {
    fn from(value: &GraphicsPipeline<C>) -> Self {
        PipelineBindData {
            bind_point: vk::PipelineBindPoint::GRAPHICS,
            pipeline: value.handle,
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

impl<T: GraphicsPipelineConfig> PipelinePack<T> {
    pub fn layout(&self) -> PipelineLayout<T::Layout> {
        unsafe { PipelineLayout::wrap(self.data.layout) }
    }

    pub fn len(&self) -> usize {
        self.data.pipelines.len()
    }

    pub fn get(&self, index: usize) -> GraphicsPipeline<T> {
        GraphicsPipeline {
            handle: self.data.pipelines[index],
            layout: self.data.layout,
            _phantom: PhantomData,
        }
    }

    pub fn insert(&mut self, pipeline: GraphicsPipeline<T>) {
        self.data.pipelines.push(pipeline.handle);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PipelinePackRef<'a, T: GraphicsPipelineConfig> {
    data: &'a PipelinePackData,
    _phantom: PhantomData<T>,
}

impl<'a, T: GraphicsPipelineConfig, N: GraphicsPipelineConfig> TryFrom<&'a PipelinePack<T>>
    for PipelinePackRef<'a, N>
{
    type Error = &'static str;

    fn try_from(value: &'a PipelinePack<T>) -> Result<Self, Self::Error> {
        if TypeId::of::<T>() == TypeId::of::<N>() {
            Ok(PipelinePackRef {
                data: &value.data,
                _phantom: PhantomData,
            })
        } else {
            Err("Invalid GraphicsPipelineConfig type!")
        }
    }
}

impl<'a, T: GraphicsPipelineConfig> PipelinePackRef<'a, T> {
    pub fn layout(&self) -> PipelineLayout<T::Layout> {
        unsafe { PipelineLayout::wrap(self.data.layout) }
    }

    pub fn len(&self) -> usize {
        self.data.pipelines.len()
    }

    pub fn get(&self, index: usize) -> GraphicsPipeline<T> {
        GraphicsPipeline {
            handle: self.data.pipelines[index],
            layout: self.data.layout,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct PipelinePackRefMut<'a, T: GraphicsPipelineConfig> {
    data: &'a mut PipelinePackData,
    _phantom: PhantomData<T>,
}

impl<'a, T: GraphicsPipelineConfig, N: GraphicsPipelineConfig> TryFrom<&'a mut PipelinePack<T>>
    for PipelinePackRefMut<'a, N>
{
    type Error = &'static str;

    fn try_from(value: &'a mut PipelinePack<T>) -> Result<Self, Self::Error> {
        if TypeId::of::<T>() == TypeId::of::<N>() {
            Ok(PipelinePackRefMut {
                data: &mut value.data,
                _phantom: PhantomData,
            })
        } else {
            Err("Invalid GraphicsPipelineConfig type!")
        }
    }
}

impl<'a, T: GraphicsPipelineConfig> PipelinePackRefMut<'a, T> {
    pub fn layout(&self) -> PipelineLayout<T::Layout> {
        unsafe { PipelineLayout::wrap(self.data.layout) }
    }

    pub fn len(&self) -> usize {
        self.data.pipelines.len()
    }

    pub fn get(&self, index: usize) -> GraphicsPipeline<T> {
        GraphicsPipeline {
            handle: self.data.pipelines[index],
            layout: self.data.layout,
            _phantom: PhantomData,
        }
    }

    pub fn insert(&mut self, pipeline: GraphicsPipeline<T>) {
        self.data.pipelines.push(pipeline.handle);
    }
}

impl Context {
    pub fn load_pipelines<S: GraphicsPipelineConfig + ModuleLoader>(
        &self,
        pack: &mut PipelinePack<S>,
        pipelines: &[S],
    ) -> VkResult<()> {
        for pipeline in pipelines.iter() {
            pack.insert(GraphicsPipeline::create(pipeline, self)?);
        }
        Ok(())
    }
}

impl<T: GraphicsPipelineConfig> Create for PipelinePack<T> {
    type Config<'a> = ();
    type CreateError = VkError;

    fn create<'a, 'b>(_: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let layout = context.get_or_create_unique_resource::<PipelineLayout<T::Layout>, _>()?;
        let data = PipelinePackData {
            pipelines: Vec::new(),
            layout: layout.get_vk_layout(),
        };
        Ok(PipelinePack {
            data,
            _phantom: PhantomData,
        })
    }
}

impl<T: GraphicsPipelineConfig> Destroy for PipelinePack<T> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.data.pipelines.iter().for_each(|&p| unsafe {
            context.destroy_pipeline(p, None);
        });
        Ok(())
    }
}
