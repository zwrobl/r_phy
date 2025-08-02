use std::{any::type_name, marker::PhantomData, mem::size_of};

use ash::vk;
use bytemuck::AnyBitPattern;

use crate::device::{
    raw::resources::{
        buffer::UniformBuffer,
        command::operation::Operation,
        image::DescriptorImageInfo,
        layout::{DescriptorBinding, DescriptorLayout, DescriptorType},
    },
    Device,
};

use super::Descriptor;

#[derive(Debug, Clone, Copy)]
pub struct DescriptorWriteInfo {
    binding: u32,
    descriptor_type: vk::DescriptorType,
    descriptor_count: u32,
    dst_array_element: u32,
}

impl DescriptorWriteInfo {
    pub fn new(binding: u32, descriptor_type: DescriptorType) -> Self {
        Self {
            binding,
            descriptor_type: descriptor_type.get_vk_descriptor_type(),
            descriptor_count: 1,
            dst_array_element: 0,
        }
    }

    pub fn with_descriptor_count(self, count: u32) -> Self {
        Self {
            descriptor_count: count,
            ..self
        }
    }

    pub fn with_dst_array_element(self, element: u32) -> Self {
        Self {
            dst_array_element: element,
            ..self
        }
    }
}

#[derive(Debug)]
enum SetWrite {
    Buffer {
        set_index: usize,
        buffer_write_index: usize,
        write: DescriptorWriteInfo,
    },
    Image {
        set_index: usize,
        image_write_index: usize,
        write: DescriptorWriteInfo,
    },
}

impl SetWrite {
    fn get_vk_write(
        &self,
        descriptors: &[vk::DescriptorSet],
        image_writes: &[vk::DescriptorImageInfo],
        buffer_writes: &[vk::DescriptorBufferInfo],
    ) -> vk::WriteDescriptorSet {
        match self {
            SetWrite::Buffer {
                set_index,
                buffer_write_index,
                write,
            } => vk::WriteDescriptorSet {
                dst_set: descriptors[*set_index],
                dst_binding: write.binding,
                descriptor_count: write.descriptor_count,
                descriptor_type: write.descriptor_type,
                dst_array_element: write.dst_array_element,
                p_buffer_info: &buffer_writes[*buffer_write_index],
                ..Default::default()
            },
            SetWrite::Image {
                set_index,
                image_write_index,
                write,
            } => vk::WriteDescriptorSet {
                dst_set: descriptors[*set_index],
                dst_binding: write.binding,
                descriptor_count: write.descriptor_count,
                descriptor_type: write.descriptor_type,
                dst_array_element: write.dst_array_element,
                p_image_info: &image_writes[*image_write_index],
                ..Default::default()
            },
        }
    }
}

#[derive(Debug)]
pub struct DescriptorSetWriter<T: DescriptorLayout> {
    num_sets: usize,
    writes: Vec<SetWrite>,
    bufer_writes: Vec<vk::DescriptorBufferInfo>,
    image_writes: Vec<vk::DescriptorImageInfo>,
    _phantom: PhantomData<T>,
}

impl<T: DescriptorLayout> DescriptorSetWriter<T> {
    pub fn new(num_sets: usize) -> DescriptorSetWriter<T> {
        DescriptorSetWriter {
            num_sets,
            writes: vec![],
            bufer_writes: vec![],
            image_writes: vec![],
            _phantom: PhantomData,
        }
    }

    pub fn num_sets(&self) -> usize {
        self.num_sets
    }

    pub fn write_buffer<U: AnyBitPattern + DescriptorBinding, O: Operation>(
        mut self,
        buffer: &UniformBuffer<U, O>,
    ) -> Self {
        let writes = T::get_descriptor_writes::<U>();
        if writes.is_empty() {
            panic!(
                "Invalid DescriptorBinding type {} for descriptor layout {}",
                type_name::<U>(),
                type_name::<T>()
            )
        }
        let descriptor_count = writes
            .iter()
            .map(|write| write.descriptor_count as usize)
            .sum::<usize>();
        let num_uniforms = self.num_sets * descriptor_count;
        debug_assert_eq!(
            num_uniforms,
            buffer.len(),
            "UniformBuffer object not large enough for DescriptorPool write!"
        );
        let buffer_write_base_index = self.bufer_writes.len();
        self.bufer_writes
            .extend((0..num_uniforms).map(|index| vk::DescriptorBufferInfo {
                buffer: buffer.get_vk_buffer(),
                offset: (size_of::<U>() * index) as vk::DeviceSize,
                range: size_of::<U>() as vk::DeviceSize,
            }));
        self.writes.extend((0..self.num_sets).flat_map(|set_index| {
            let mut buffer_set_write_offset = 0;
            writes
                .iter()
                .map(|&write| {
                    let descriptor_write = SetWrite::Buffer {
                        set_index,
                        buffer_write_index: buffer_write_base_index
                            + set_index * descriptor_count
                            + buffer_set_write_offset,
                        write,
                    };
                    buffer_set_write_offset += write.descriptor_count as usize;
                    descriptor_write
                })
                .collect::<Vec<_>>()
        }));
        self
    }

    pub fn write_images<'a, B>(mut self, images: &'a [DescriptorImageInfo]) -> Self
    where
        B: DescriptorBinding,
    {
        let writes = T::get_descriptor_writes::<B>();
        if writes.is_empty() {
            panic!(
                "Invalid DescriptorBinding type {} for descriptor layout {}",
                type_name::<B>(),
                type_name::<T>()
            )
        }
        let descciptor_count = writes
            .iter()
            .map(|write| write.descriptor_count as usize)
            .sum::<usize>();
        let num_uniforms = self.num_sets * descciptor_count;
        debug_assert_eq!(
            num_uniforms,
            images.len(),
            "Not enough images for DescriptorPool write!"
        );
        let iamge_write_base_index = self.image_writes.len();
        self.image_writes.extend(
            images
                .iter()
                .map(|image| image.get_vk_descriptor_image_info()),
        );
        self.writes.extend((0..self.num_sets).flat_map(|set_index| {
            let mut image_set_write_offset = 0;
            writes
                .iter()
                .map(|&write| {
                    let descriptor_write = SetWrite::Image {
                        set_index,
                        image_write_index: iamge_write_base_index
                            + set_index * descciptor_count
                            + image_set_write_offset,
                        write,
                    };
                    image_set_write_offset += write.descriptor_count as usize;
                    descriptor_write
                })
                .collect::<Vec<_>>()
        }));
        self
    }
}

impl Device {
    // TODO: sets Vec of incorrect length could be passed here
    pub fn write_descriptors<T: DescriptorLayout>(
        &self,
        writer: DescriptorSetWriter<T>,
        sets: Vec<vk::DescriptorSet>,
    ) -> Vec<Descriptor<T>> {
        let DescriptorSetWriter {
            writes,
            bufer_writes,
            image_writes,
            ..
        } = writer;
        let writes = writes
            .into_iter()
            .map(|write| write.get_vk_write(&sets, &image_writes, &bufer_writes))
            .collect::<Vec<_>>();
        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
        sets.into_iter()
            .map(|set| Descriptor {
                set,
                _phantom: PhantomData,
            })
            .collect()
    }
}
