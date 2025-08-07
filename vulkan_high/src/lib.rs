pub mod context;
pub mod renderer;
pub mod resources;

use graphics::renderer::{Renderer, RendererBuilder};
use std::error::Error;
use std::ops::Deref;
use std::rc::Rc;
use winit::window::Window;

use crate::context::VulkanContext;

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
pub struct VulkanRendererBuilder {
    config: Option<VulkanRendererConfig>,
}

impl Default for VulkanRendererBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl VulkanRendererBuilder {
    pub fn new() -> Self {
        Self { config: None }
    }

    pub fn with_config(mut self, config: VulkanRendererConfig) -> Self {
        self.config = Some(config);
        self
    }
}

impl RendererBuilder for VulkanRendererBuilder {
    type Renderer = VulkanRenderer;

    fn build(self, window: &Window) -> Result<Self::Renderer, Box<dyn Error>> {
        let renderer =
            VulkanRenderer::new(window, self.config.ok_or("Configuration not provided")?)?;
        Ok(renderer)
    }
}

pub struct VulkanRenderer {
    context: Rc<VulkanContext>,
}

impl Deref for VulkanRenderer {
    type Target = VulkanContext;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl VulkanRenderer {
    pub fn new(window: &Window, config: VulkanRendererConfig) -> Result<Self, Box<dyn Error>> {
        let context = VulkanContext::new(window, config)?;
        Ok(Self {
            context: Rc::new(context),
        })
    }
}

impl Renderer for VulkanRenderer {}
