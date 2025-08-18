pub mod context;
pub mod error;
pub mod renderer;
pub mod resources;

use graphics::error::GraphicsResult;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use type_kit::{Create, Destroy};
use vulkan_low::Context;
use vulkan_low::memory::allocator::StaticConfig;
use vulkan_low::memory::allocator::{AllocatorIndexTyped, Static};
use vulkan_low::resources::Partial;
use winit::window::Window;

use crate::context::VulkanContextBuilder;
use crate::renderer::{Renderer, RendererBuilder};
use crate::resources::{CommonResources, CommonResourcesPartial};

#[derive(Debug, Clone, Copy)]
pub struct VulkanRendererConfig {}

#[derive(Debug, Clone, Copy, Default)]
pub struct VulkanRendererConfigBuilder {}

impl VulkanRendererConfig {
    pub fn builder() -> VulkanRendererConfigBuilder {
        VulkanRendererConfigBuilder::default()
    }
}

impl VulkanRendererConfigBuilder {
    pub fn build(self) -> GraphicsResult<VulkanRendererConfig> {
        let config = VulkanRendererConfig {};
        Ok(config)
    }
}

#[derive(Debug)]
pub struct VulkanRendererBuilder<R: RendererBuilder> {
    config: Option<VulkanRendererConfig>,
    renderer: R::Config,
}

impl<R: RendererBuilder> VulkanRendererBuilder<R> {
    pub fn new(config: R::Config) -> Self {
        Self {
            config: None,
            renderer: config,
        }
    }

    pub fn with_config(mut self, config: VulkanRendererConfig) -> Self {
        self.config = Some(config);
        self
    }
}

pub struct VulkanContext {
    context: Context,
    common_resources: CommonResources,
    allocator: AllocatorIndexTyped<Static>,
    _config: VulkanRendererConfig,
}

impl VulkanContext {
    #[inline]
    pub fn common_resources(&self) -> &CommonResources {
        &self.common_resources
    }
}

impl VulkanContext {
    pub fn new(window: &Window, config: VulkanRendererConfig) -> GraphicsResult<Rc<Self>> {
        let context = Context::build(window)?;
        let common_resources = CommonResourcesPartial::create((), &context)?;
        let mut allocator_config = StaticConfig::new();
        common_resources.register_memory_requirements(&mut allocator_config);
        let allocator = context.create_allocator::<Static, _>(allocator_config)?;
        let common_resources =
            CommonResources::create((common_resources, allocator.into()), &context)?;
        let context = VulkanContext {
            context,
            common_resources,
            allocator,
            _config: config,
        };
        Ok(Rc::new(context))
    }
}

impl Deref for VulkanContext {
    type Target = Context;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        let _ = self.context.wait_idle();
        let _ = self.common_resources.destroy(&self.context);
        let _ = self.context.destroy_allocator(self.allocator);
    }
}

impl<R: RendererBuilder> graphics::renderer::RendererBuilder for VulkanRendererBuilder<R> {
    fn build(self, window: &Window) -> GraphicsResult<impl graphics::renderer::Renderer + use<R>> {
        let Self { config, renderer } = self;
        let context = VulkanContext::new(window, config.unwrap())?;
        let mut allocator = StaticConfig::new();
        let renderer = R::new(&context, renderer)?;
        renderer.register_memory_requirements(&mut allocator);
        let allocator = context.create_allocator(allocator)?;
        let renderer = renderer.with_allocator(allocator).build()?;
        Ok(VulkanRenderer {
            context,
            allocator,
            renderer,
        })
    }
}

pub struct VulkanRenderer<R: Renderer> {
    allocator: AllocatorIndexTyped<Static>,
    context: Rc<VulkanContext>,
    renderer: R,
}

impl<R: Renderer> Drop for VulkanRenderer<R> {
    #[inline]
    fn drop(&mut self) {
        let _ = self.renderer.destroy(&self.context);
        let _ = self.context.destroy_allocator(self.allocator);
    }
}

impl<R: Renderer> VulkanRenderer<R> {
    pub fn context(&self) -> &VulkanContext {
        &self.context
    }

    pub fn shared_context(&self) -> Rc<VulkanContext> {
        self.context.clone()
    }
}

impl<R: Renderer> Deref for VulkanRenderer<R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.renderer
    }
}

impl<R: Renderer> DerefMut for VulkanRenderer<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.renderer
    }
}

impl<R: Renderer> graphics::renderer::Renderer for VulkanRenderer<R> {
    fn context_builder() -> impl graphics::renderer::ContextBuilder<Renderer = Self> {
        VulkanContextBuilder::new()
    }
}
