mod list;
mod pack;

use std::marker::PhantomData;

pub use list::*;
pub use pack::*;

use ash::vk;
use graphics::model::Material as MaterialBase;
use type_kit::{Cons, Nil};

use crate::context::device::raw::resources::layout::{
    presets::{FragmentStage, PodUniform},
    DescriptorBinding, DescriptorLayout, DescriptorLayoutBuilder,
};

pub struct TextureSamplers<M: Material> {
    _phantom: PhantomData<M>,
}

impl<T: Material> DescriptorBinding for TextureSamplers<T> {
    fn has_data() -> bool {
        T::NUM_IMAGES > 0
    }

    fn get_descriptor_set_binding(binding: u32) -> ash::vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: T::NUM_IMAGES as u32,
            stage_flags: ash::vk::ShaderStageFlags::FRAGMENT,
            p_immutable_samplers: std::ptr::null(),
        }
    }

    fn get_descriptor_write(binding: u32) -> ash::vk::WriteDescriptorSet {
        ash::vk::WriteDescriptorSet {
            dst_binding: binding,
            dst_array_element: 0,
            descriptor_count: T::NUM_IMAGES as u32,
            descriptor_type: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            ..Default::default()
        }
    }

    fn get_descriptor_pool_size(num_sets: u32) -> ash::vk::DescriptorPoolSize {
        ash::vk::DescriptorPoolSize {
            ty: ash::vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: num_sets * T::NUM_IMAGES as u32,
        }
    }
}

pub trait Material: MaterialBase {
    type DescriptorLayout: DescriptorLayout;
}

impl<T: MaterialBase> Material for T {
    type DescriptorLayout = DescriptorLayoutBuilder<
        Cons<PodUniform<T::Uniform, FragmentStage>, Cons<TextureSamplers<T>, Nil>>,
    >;
}
