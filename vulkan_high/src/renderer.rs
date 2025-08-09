use std::{convert::Infallible, ops::Deref, rc::Rc};

use graphics::{renderer::camera::CameraMatrices, shader::ShaderType};
use type_kit::{list_type, Cons, Destroy, TypedNil};
use vulkan_low::{
    memory::allocator::AllocatorIndex,
    resources::{
        descriptor::{Descriptor, DescriptorSetMapper},
        error::ResourceResult,
        layout::{presets::CameraDescriptorSet, DescriptorLayout},
        pipeline::{GraphicsPipelineConfig, ModuleLoader},
        Partial,
    },
    Context,
};

use crate::{
    renderer::{frame::FrameCell, storage::DrawStorage},
    VulkanContext,
};

pub mod deferred;
pub mod frame;
pub mod storage;

pub type FrameData<C> = list_type![FrameCell<C>, DrawStorage, TypedNil<DestroyTerminator>];

pub struct ExternalResources {
    context: Rc<VulkanContext>,
}

impl ExternalResources {
    pub fn new(context: &Rc<VulkanContext>) -> Self {
        Self {
            context: context.clone(),
        }
    }
}

impl Deref for ExternalResources {
    type Target = VulkanContext;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl Destroy for ExternalResources {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, _context: &Context) -> Result<(), Self::DestroyError> {
        Ok(())
    }
}

pub trait ShaderDescriptor<T: DescriptorLayout>: GraphicsPipelineConfig {
    fn get_mapper() -> DescriptorSetMapper<T, Self::Layout>;
}

pub trait Renderer: for<'a> Destroy<Context<'a> = &'a Context, DestroyError = Infallible> {
    type ShaderType<T: ShaderType>: ShaderDescriptor<CameraDescriptorSet>
        + ShaderType<Vertex = T::Vertex, Material = T::Material>
        + ModuleLoader
        + From<T>;

    fn begin_frame(
        &mut self,
        context: &Context,
        camera: CameraMatrices,
    ) -> ResourceResult<Descriptor<CameraDescriptorSet>>;

    fn render(&mut self, context: &Context, draw_calls: DrawStorage) -> ResourceResult<()>;
}

pub trait RendererBuilder:
    Partial + for<'a> Destroy<Context<'a> = &'a Context, DestroyError = Infallible>
{
    type Config;

    fn new(context: &Rc<VulkanContext>, config: Self::Config) -> ResourceResult<Self>;

    fn with_allocator<T: Into<AllocatorIndex>>(self, allocator: T) -> Self;

    fn build<'a>(self) -> ResourceResult<impl Renderer>;
}

pub struct DestroyTerminator;

impl Destroy for DestroyTerminator {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, _context: &Context) -> Result<(), Self::DestroyError> {
        Ok(())
    }
}
