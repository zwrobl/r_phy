use std::any::type_name;

use graphics::shader::ShaderType;
use type_kit::{Cons, Create, Destroy, Nil, TypeList};
use vulkan_low::{Context, error::VkResult, resources::pipeline::GraphicsPipelineConfig};

use crate::renderer::Renderer;

use super::{PipelinePack, PipelinePackRef};

pub trait GraphicsPipelineListBuilder: TypeList {
    type Pack<R: Renderer>: GraphicsPipelinePackList;

    fn build<R: Renderer>(self, context: &Context) -> VkResult<Self::Pack<R>>;
}

impl GraphicsPipelineListBuilder for Nil {
    type Pack<R: Renderer> = Nil;

    fn build<R: Renderer>(self, _context: &Context) -> VkResult<Self::Pack<R>> {
        Ok(Nil::new())
    }
}

impl<T: ShaderType, N: GraphicsPipelineListBuilder> GraphicsPipelineListBuilder
    for Cons<Vec<T>, N>
{
    type Pack<R: Renderer> = Cons<PipelinePack<R::ShaderType<T>>, N::Pack<R>>;

    fn build<R: Renderer>(self, context: &Context) -> VkResult<Self::Pack<R>> {
        let Cons { head, tail } = self;
        let shaders = head
            .into_iter()
            .map(|shader| shader.into())
            .collect::<Vec<R::ShaderType<T>>>();
        let pack = PipelinePack::create(&shaders, context)?;
        Ok(Cons {
            head: pack,
            tail: tail.build(context)?,
        })
    }
}

pub trait GraphicsPipelinePackList: TypeList + 'static {
    fn destroy(&mut self, _context: &Context);

    fn try_get<P: GraphicsPipelineConfig>(&self) -> Option<PipelinePackRef<'_, P>>;
    fn get<P: GraphicsPipelineConfig>(&self) -> PipelinePackRef<'_, P>;
}

impl GraphicsPipelinePackList for Nil {
    fn destroy(&mut self, _context: &Context) {}

    fn try_get<P: GraphicsPipelineConfig>(&self) -> Option<PipelinePackRef<'_, P>> {
        None
    }

    fn get<P: GraphicsPipelineConfig>(&self) -> PipelinePackRef<'_, P> {
        panic!(
            "No pipeline pack found for the requested type: {}",
            type_name::<P>()
        );
    }
}

impl<T: GraphicsPipelineConfig + ShaderType, N: GraphicsPipelinePackList> GraphicsPipelinePackList
    for Cons<PipelinePack<T>, N>
{
    fn destroy(&mut self, context: &Context) {
        let _ = self.head.destroy(context);
        self.tail.destroy(context);
    }

    fn try_get<P: GraphicsPipelineConfig>(&self) -> Option<PipelinePackRef<'_, P>> {
        if let Ok(pipelines) = (&self.head).try_into() {
            Some(pipelines)
        } else {
            self.tail.try_get::<P>()
        }
    }

    fn get<P: GraphicsPipelineConfig>(&self) -> PipelinePackRef<'_, P> {
        if let Ok(pipelines) = (&self.head).try_into() {
            pipelines
        } else {
            self.tail.get::<P>()
        }
    }
}
