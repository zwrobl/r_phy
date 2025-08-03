use std::{any::TypeId, convert::Infallible, marker::PhantomData};

use type_kit::{Create, CreateResult, Destroy, DestroyResult, FromGuard};
use vulkan_low::device::raw::resources::pipeline::{GraphicsPipeline, GraphicsPipelineConfig};

use vulkan_low::device::raw::resources::{RawIndex, ResourceIndex};
use vulkan_low::error::VkError;
use vulkan_low::{device::raw::resources::pipeline::ModuleLoader, Context};

#[derive(Debug)]
pub struct PipelinePack<T: GraphicsPipelineConfig> {
    pipelines: Vec<RawIndex>,
    _phantom: PhantomData<T>,
}

impl<T: GraphicsPipelineConfig> PipelinePack<T> {
    pub fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pipelines.len()
    }

    pub fn get(&self, index: usize) -> ResourceIndex<GraphicsPipeline<T>> {
        unsafe { ResourceIndex::<GraphicsPipeline<T>>::from_inner(self.pipelines[index]) }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PipelinePackRef<'a, T: GraphicsPipelineConfig> {
    pipelines: &'a [RawIndex],
    _phantom: PhantomData<T>,
}

impl<'a, T: GraphicsPipelineConfig, N: GraphicsPipelineConfig> TryFrom<&'a PipelinePack<T>>
    for PipelinePackRef<'a, N>
{
    type Error = &'static str;

    fn try_from(value: &'a PipelinePack<T>) -> Result<Self, Self::Error> {
        if TypeId::of::<T>() == TypeId::of::<N>() {
            Ok(PipelinePackRef {
                pipelines: &value.pipelines,
                _phantom: PhantomData,
            })
        } else {
            Err("Invalid GraphicsPipelineConfig type!")
        }
    }
}

impl<'a, T: GraphicsPipelineConfig> PipelinePackRef<'a, T> {
    pub fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pipelines.len()
    }

    pub fn get(&self, index: usize) -> ResourceIndex<GraphicsPipeline<T>> {
        unsafe { ResourceIndex::<GraphicsPipeline<T>>::from_inner(self.pipelines[index]) }
    }
}

impl<T: GraphicsPipelineConfig + ModuleLoader> Create for PipelinePack<T> {
    type Config<'a> = &'a [T];
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let pipelines = config
            .iter()
            .map(|pipeline| {
                context
                    .create_resource::<GraphicsPipeline<T>, _>(pipeline)
                    .map(|pipeline| pipeline.into_inner())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PipelinePack {
            pipelines,
            _phantom: PhantomData,
        })
    }
}

impl<T: GraphicsPipelineConfig> Destroy for PipelinePack<T> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        (0..self.pipelines.len()).for_each(|i| {
            let resource_index = self.get(i);
            let _ = context.destroy_resource(resource_index);
        });
        Ok(())
    }
}
