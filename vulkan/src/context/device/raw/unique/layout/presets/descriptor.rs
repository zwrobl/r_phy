use std::marker::PhantomData;

use ash::vk;
use bytemuck::{AnyBitPattern, Zeroable};

use crate::context::device::{
    framebuffer::InputAttachment,
    raw::{
        resources::image::{Image2D, Texture},
        unique::layout::{DescriptorBinding, DescriptorLayoutBuilder},
    },
};
use graphics::renderer::camera::CameraMatrices;
use type_kit::{Cons, Nil};

pub trait PipelineStage: 'static {
    const STAGE: vk::ShaderStageFlags;
}

#[repr(C)]
#[derive(Debug)]
pub struct VertexStage;

impl PipelineStage for VertexStage {
    const STAGE: vk::ShaderStageFlags = vk::ShaderStageFlags::VERTEX;
}

#[repr(C)]
#[derive(Debug)]
pub struct FragmentStage;

impl PipelineStage for FragmentStage {
    const STAGE: vk::ShaderStageFlags = vk::ShaderStageFlags::FRAGMENT;
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

    fn get_descriptor_set_binding(binding: u32) -> vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
            stage_flags: S::STAGE,
            p_immutable_samplers: std::ptr::null(),
        }
    }

    fn get_descriptor_write(binding: u32) -> vk::WriteDescriptorSet {
        vk::WriteDescriptorSet {
            dst_binding: binding,
            dst_array_element: 0,
            descriptor_count: 1,
            descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
            ..Default::default()
        }
    }

    fn get_descriptor_pool_size(num_sets: u32) -> vk::DescriptorPoolSize {
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: num_sets,
        }
    }
}

impl DescriptorBinding for CameraMatrices {
    fn has_data() -> bool {
        true
    }

    fn get_descriptor_set_binding(binding: u32) -> vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::VERTEX,
            p_immutable_samplers: std::ptr::null(),
        }
    }

    fn get_descriptor_write(binding: u32) -> vk::WriteDescriptorSet {
        vk::WriteDescriptorSet {
            dst_binding: binding,
            dst_array_element: 0,
            descriptor_count: 1,
            descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
            ..Default::default()
        }
    }

    fn get_descriptor_pool_size(num_sets: u32) -> vk::DescriptorPoolSize {
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: num_sets,
        }
    }
}

impl DescriptorBinding for Texture<Image2D> {
    fn has_data() -> bool {
        true
    }

    fn get_descriptor_set_binding(binding: u32) -> vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::FRAGMENT,
            p_immutable_samplers: std::ptr::null(),
        }
    }

    fn get_descriptor_write(binding: u32) -> vk::WriteDescriptorSet {
        vk::WriteDescriptorSet {
            dst_binding: binding,
            dst_array_element: 0,
            descriptor_count: 1,
            descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            ..Default::default()
        }
    }

    fn get_descriptor_pool_size(num_sets: u32) -> vk::DescriptorPoolSize {
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: num_sets,
        }
    }
}

impl DescriptorBinding for InputAttachment {
    fn has_data() -> bool {
        true
    }

    fn get_descriptor_set_binding(binding: u32) -> vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type: vk::DescriptorType::INPUT_ATTACHMENT,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::FRAGMENT,
            p_immutable_samplers: std::ptr::null(),
        }
    }

    fn get_descriptor_write(binding: u32) -> vk::WriteDescriptorSet {
        vk::WriteDescriptorSet {
            dst_binding: binding,
            dst_array_element: 0,
            descriptor_count: 1,
            descriptor_type: vk::DescriptorType::INPUT_ATTACHMENT,
            ..Default::default()
        }
    }

    fn get_descriptor_pool_size(num_sets: u32) -> vk::DescriptorPoolSize {
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::INPUT_ATTACHMENT,
            descriptor_count: num_sets,
        }
    }
}

pub type CameraDescriptorSet = DescriptorLayoutBuilder<Cons<CameraMatrices, Nil>>;

pub type TextureDescriptorSet = DescriptorLayoutBuilder<Cons<Texture<Image2D>, Nil>>;

pub type GBufferDescriptorSet = DescriptorLayoutBuilder<
    Cons<
        // Albedo
        InputAttachment,
        Cons<
            // Position
            InputAttachment,
            Cons<
                // Normal
                InputAttachment,
                Cons<
                    // Depth
                    InputAttachment,
                    Nil,
                >,
            >,
        >,
    >,
>;
