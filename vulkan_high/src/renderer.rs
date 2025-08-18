use std::{convert::Infallible, ops::Deref, rc::Rc};

use graphics::{renderer::camera::CameraMatrices, shader::ShaderType};
use type_kit::Destroy;
use vulkan_low::{
    Context,
    memory::allocator::AllocatorIndex,
    resources::{
        Partial, Resource, ResourceIndex,
        descriptor::{Descriptor, DescriptorSetMapper},
        error::ResourceResult,
        layout::{DescriptorLayout, presets::CameraDescriptorSet},
        pipeline::{GraphicsPipelineConfig, ModuleLoader},
    },
};

use crate::{VulkanContext, renderer::storage::DrawStorage, resources::GraphicsPipelinePackList};

pub mod deferred;
pub mod frame;
pub mod storage;

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

pub trait Renderer:
    for<'a> Destroy<Context<'a> = &'a Context, DestroyError = Infallible> + 'static
{
    type ShaderType<T: ShaderType>: ShaderDescriptor<CameraDescriptorSet>
        + ShaderType<Vertex = T::Vertex, Material = T::Material>
        + ModuleLoader
        + From<T>;

    type RendererContext<'b, P: GraphicsPipelinePackList>: RendererContext;

    fn load_context<'a, P: GraphicsPipelinePackList>(
        &'a mut self,
        context: &Context,
    ) -> ResourceResult<Self::RendererContext<'a, P>>;
}

pub trait RendererContext:
    for<'a> Destroy<Context<'a> = &'a Context, DestroyError = Infallible>
{
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

    fn build(self) -> ResourceResult<impl Renderer>;
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

#[derive(Debug)]
pub struct ResourceCell<T: Resource> {
    resource: Option<ResourceIndex<T>>,
}

impl<T: Resource> ResourceCell<T> {
    #[inline]
    pub fn empty() -> Self {
        Self { resource: None }
    }

    #[inline]
    pub fn new(resource: ResourceIndex<T>) -> Self {
        Self {
            resource: Some(resource),
        }
    }

    #[inline]
    pub fn index(&self) -> ResourceIndex<T> {
        self.resource.expect("ResourceCell is empty")
    }
}

impl<T: Resource> Destroy for ResourceCell<T> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, _context: &Context) -> Result<(), Self::DestroyError> {
        // ResourceCell<T> does not own the resource
        let _ = self.resource.take();
        Ok(())
    }
}
