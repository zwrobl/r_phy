pub mod context;
pub mod renderer;
pub mod resources;

use graphics::renderer::camera::CameraMatrices;
use std::error::Error;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use vulkan_low::memory::allocator::StaticConfig;
use vulkan_low::memory::allocator::{AllocatorIndexTyped, Static};
use vulkan_low::resources::descriptor::Descriptor;
use vulkan_low::resources::error::ResourceResult;
use vulkan_low::resources::layout::presets::CameraDescriptorSet;
use winit::window::Window;

use crate::context::{VulkanContext, VulkanContextBuilder};
use crate::renderer::storage::DrawStorage;
use crate::renderer::{Renderer, RendererBuilder};

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
    pub fn build(self) -> Result<VulkanRendererConfig, Box<dyn Error>> {
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

impl<R: RendererBuilder> graphics::renderer::RendererBuilder for VulkanRendererBuilder<R> {
    fn build(
        self,
        window: Rc<Window>,
    ) -> Result<impl graphics::renderer::Renderer, Box<dyn Error>> {
        let Self { config, renderer } = self;
        let context = VulkanContext::new(&window, config.unwrap())?;
        let mut allocator = StaticConfig::new();
        let renderer = R::new(&context, renderer)?;
        renderer.register_memory_requirements(&mut allocator);
        let allocator = context.create_allocator(allocator)?;
        let renderer = renderer.with_allocator(allocator).build()?;
        Ok(VulkanRenderer {
            _window: window,
            context,
            allocator,
            renderer,
        })
    }
}

pub struct VulkanRenderer<R: Renderer> {
    _window: Rc<Window>,
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

    #[inline]
    fn begin_frame(
        &mut self,
        camera: CameraMatrices,
    ) -> ResourceResult<Descriptor<CameraDescriptorSet>> {
        self.renderer.begin_frame(&self.context, camera)
    }

    #[inline]
    fn end_frame(&mut self, draw_storage: DrawStorage) -> ResourceResult<()> {
        self.renderer.render(&self.context, draw_storage)
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
