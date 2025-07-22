use crate::context::{device::pipeline::ModuleLoader, error::VkResult, Context};
use graphics::shader::ShaderType;
use type_kit::{Cons, Create, Destroy, Nil, TypeList};

use super::{GraphicsPipelineConfig, PipelinePack, PipelinePackRef, PipelinePackRefMut};

pub trait GraphicsPipelineListBuilder: TypeList {
    type Pack: GraphicsPipelinePackList;

    fn build(&self, context: &Context) -> VkResult<Self::Pack>;
}

impl GraphicsPipelineListBuilder for Nil {
    type Pack = Nil;

    fn build(&self, _context: &Context) -> VkResult<Self::Pack> {
        Ok(Nil::new())
    }
}

impl<T: GraphicsPipelineConfig + ModuleLoader + ShaderType, N: GraphicsPipelineListBuilder>
    GraphicsPipelineListBuilder for Cons<Vec<T>, N>
{
    type Pack = Cons<PipelinePack<T>, N::Pack>;

    fn build(&self, context: &Context) -> VkResult<Self::Pack> {
        let mut pack = PipelinePack::create((), context)?;
        context.load_pipelines(&mut pack, &self.head)?;
        Ok(Cons {
            head: pack,
            tail: self.tail.build(context)?,
        })
    }
}

pub trait GraphicsPipelinePackList: TypeList + 'static {
    fn destroy(&mut self, _context: &Context);

    fn try_get<P: GraphicsPipelineConfig>(&self) -> Option<PipelinePackRef<P>>;

    fn try_get_mut<P: GraphicsPipelineConfig>(&mut self) -> Option<PipelinePackRefMut<P>>;
}

impl GraphicsPipelinePackList for Nil {
    fn destroy(&mut self, _context: &Context) {}

    fn try_get<P: GraphicsPipelineConfig>(&self) -> Option<PipelinePackRef<P>> {
        None
    }

    fn try_get_mut<P: GraphicsPipelineConfig>(&mut self) -> Option<PipelinePackRefMut<P>> {
        None
    }
}

impl<T: GraphicsPipelineConfig + ShaderType, N: GraphicsPipelinePackList> GraphicsPipelinePackList
    for Cons<PipelinePack<T>, N>
{
    fn destroy(&mut self, context: &Context) {
        let _ = self.head.destroy(context);
        self.tail.destroy(context);
    }

    fn try_get<P: GraphicsPipelineConfig>(&self) -> Option<PipelinePackRef<P>> {
        if let Ok(pipelines) = (&self.head).try_into() {
            Some(pipelines)
        } else {
            self.tail.try_get::<P>()
        }
    }

    fn try_get_mut<P: GraphicsPipelineConfig>(&mut self) -> Option<PipelinePackRefMut<P>> {
        if let Ok(pipelines) = (&mut self.head).try_into() {
            Some(pipelines)
        } else {
            self.tail.try_get_mut::<P>()
        }
    }
}
