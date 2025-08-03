use std::marker::PhantomData;

use bytemuck::{AnyBitPattern, Zeroable};

use crate::resources::{
    descriptor::DescriptorWriteInfo,
    framebuffer::InputAttachment,
    image::{Image2D, Texture},
    layout::{
        DescriptorBinding, DescriptorLayoutBuilder, DescriptorPoolSize, DescriptorSetLayoutBinding,
        DescriptorType, ShaderStage,
    },
};
use graphics::renderer::camera::CameraMatrices;
use type_kit::{Cons, Nil};

pub trait PipelineStage: 'static {
    const STAGE: ShaderStage;
}

#[repr(C)]
#[derive(Debug)]
pub struct VertexStage;

impl PipelineStage for VertexStage {
    const STAGE: ShaderStage = ShaderStage::Vertex;
}

#[repr(C)]
#[derive(Debug)]
pub struct FragmentStage;

impl PipelineStage for FragmentStage {
    const STAGE: ShaderStage = ShaderStage::Fragment;
}

#[repr(C)]
#[derive(Debug)]
pub struct PodUniform<T: Clone + Copy + AnyBitPattern, S: PipelineStage> {
    pub data: T,
    _phantom: PhantomData<S>,
}

unsafe impl<T: Clone + Copy + AnyBitPattern, S: PipelineStage> Zeroable for PodUniform<T, S> {}

unsafe impl<T: Clone + Copy + AnyBitPattern, S: PipelineStage> AnyBitPattern for PodUniform<T, S> {}

impl<T: Clone + Copy + AnyBitPattern, S: PipelineStage> Clone for PodUniform<T, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + Copy + AnyBitPattern, S: PipelineStage> Copy for PodUniform<T, S> {}

impl<T: Clone + Copy + AnyBitPattern, S: PipelineStage> From<T> for PodUniform<T, S> {
    fn from(data: T) -> Self {
        Self {
            data,
            _phantom: PhantomData,
        }
    }
}

impl<T: Clone + Copy + AnyBitPattern, S: PipelineStage> PodUniform<T, S> {
    pub fn as_inner_ref(&self) -> &T {
        &self.data
    }

    pub fn as_inner_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T: Clone + Copy + AnyBitPattern, S: PipelineStage> DescriptorBinding for PodUniform<T, S> {
    fn has_data() -> bool {
        size_of::<Self>() > 0
    }

    fn get_descriptor_set_binding(binding: u32) -> DescriptorSetLayoutBinding {
        DescriptorType::UniformBuffer
            .layout_binding(binding)
            .with_shader_stage(S::STAGE)
    }

    fn get_descriptor_write(binding: u32) -> DescriptorWriteInfo {
        DescriptorType::UniformBuffer.write_info(binding)
    }

    fn get_descriptor_pool(num_sets: u32) -> DescriptorPoolSize {
        DescriptorType::UniformBuffer.pool_size(num_sets)
    }
}

impl DescriptorBinding for CameraMatrices {
    fn has_data() -> bool {
        true
    }

    fn get_descriptor_set_binding(binding: u32) -> DescriptorSetLayoutBinding {
        DescriptorType::UniformBuffer
            .layout_binding(binding)
            .with_shader_stage(ShaderStage::Vertex)
    }

    fn get_descriptor_write(binding: u32) -> DescriptorWriteInfo {
        DescriptorType::UniformBuffer.write_info(binding)
    }

    fn get_descriptor_pool(num_sets: u32) -> DescriptorPoolSize {
        DescriptorType::UniformBuffer.pool_size(num_sets)
    }
}

impl DescriptorBinding for Texture<Image2D> {
    fn has_data() -> bool {
        true
    }

    fn get_descriptor_set_binding(binding: u32) -> DescriptorSetLayoutBinding {
        DescriptorType::CombinedImageSampler
            .layout_binding(binding)
            .with_shader_stage(ShaderStage::Fragment)
    }

    fn get_descriptor_write(binding: u32) -> DescriptorWriteInfo {
        DescriptorType::CombinedImageSampler.write_info(binding)
    }

    fn get_descriptor_pool(num_sets: u32) -> DescriptorPoolSize {
        DescriptorType::CombinedImageSampler.pool_size(num_sets)
    }
}

impl DescriptorBinding for InputAttachment {
    fn has_data() -> bool {
        true
    }

    fn get_descriptor_set_binding(binding: u32) -> DescriptorSetLayoutBinding {
        DescriptorType::InputAttachment
            .layout_binding(binding)
            .with_shader_stage(ShaderStage::Fragment)
    }

    fn get_descriptor_write(binding: u32) -> DescriptorWriteInfo {
        DescriptorType::InputAttachment.write_info(binding)
    }

    fn get_descriptor_pool(num_sets: u32) -> DescriptorPoolSize {
        DescriptorType::InputAttachment.pool_size(num_sets)
    }
}

pub type CameraDescriptorSet = DescriptorLayoutBuilder<Cons<CameraMatrices, Nil>>;

pub type TextureDescriptorSet = DescriptorLayoutBuilder<Cons<Texture<Image2D>, Nil>>;
