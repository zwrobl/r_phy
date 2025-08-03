mod list;
mod pack;

use std::marker::PhantomData;

pub use list::*;
pub use pack::*;

use graphics::model::Material as MaterialBase;
use type_kit::{Cons, Nil};

use vulkan_low::resources::{
    descriptor::DescriptorWriteInfo,
    layout::{
        presets::{FragmentStage, PodUniform},
        DescriptorBinding, DescriptorLayout, DescriptorLayoutBuilder, DescriptorPoolSize,
        DescriptorSetLayoutBinding, DescriptorType, ShaderStage,
    },
};

pub struct TextureSamplers<M: Material> {
    _phantom: PhantomData<M>,
}

impl<T: Material> DescriptorBinding for TextureSamplers<T> {
    fn has_data() -> bool {
        T::NUM_IMAGES > 0
    }

    fn get_descriptor_set_binding(binding: u32) -> DescriptorSetLayoutBinding {
        DescriptorType::CombinedImageSampler
            .layout_binding(binding)
            .with_descriptor_count(T::NUM_IMAGES as u32)
            .with_shader_stage(ShaderStage::Fragment)
    }

    fn get_descriptor_write(binding: u32) -> DescriptorWriteInfo {
        DescriptorType::CombinedImageSampler
            .write_info(binding)
            .with_descriptor_count(T::NUM_IMAGES as u32)
    }

    fn get_descriptor_pool(num_sets: u32) -> DescriptorPoolSize {
        DescriptorType::CombinedImageSampler.pool_size(num_sets * T::NUM_IMAGES as u32)
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
