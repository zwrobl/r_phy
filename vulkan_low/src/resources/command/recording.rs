use ash::vk;
use bytemuck::{bytes_of, Pod};

use crate::{
    device::Device,
    memory::{range::ByteRange, MemoryProperties},
    resources::{
        buffer::Buffer,
        command::{BeginCommand, Command, FinishedCommand, Level, Lifetime, Operation, Secondary},
        descriptor::DescriptorBindingData,
        framebuffer::Clear,
        image::{Image, ImageType},
        layout::PushConstant,
        pipeline::{PipelineBindData, PushConstantDataRef},
        render_pass::{RenderPass, RenderPassConfig},
        swapchain::SwapchainFrame,
    },
};

pub struct RecordingCommand<'a, T: Lifetime, L: Level, O: Operation>(Command<T, L, O>, &'a Device);

impl<'a, T: Lifetime, L: Level, O: Operation> From<&'a RecordingCommand<'a, T, L, O>>
    for &'a Command<T, L, O>
{
    fn from(value: &'a RecordingCommand<T, L, O>) -> Self {
        &value.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BufferBinding {
    pub buffer: vk::Buffer,
    pub range: ByteRange,
}

#[derive(Debug, Clone, Copy)]
pub enum IndexType {
    U16,
    U32,
}

impl IndexType {
    pub fn get_vk_index_type(self) -> vk::IndexType {
        match self {
            IndexType::U16 => vk::IndexType::UINT16,
            IndexType::U32 => vk::IndexType::UINT32,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DrawIndexed {
    pub index_count: u32,
    pub index_offset: u32,
    pub vertex_offset: i32,
}

impl Device {
    pub fn start_recording<T: Lifetime, L: Level, O: Operation>(
        &self,
        command: BeginCommand<T, L, O>,
    ) -> RecordingCommand<T, L, O> {
        let BeginCommand(command) = command;
        RecordingCommand(command, self)
    }
}

impl<'a, T: Lifetime, L: Level, O: Operation> RecordingCommand<'a, T, L, O> {
    pub fn stop_recording(self) -> BeginCommand<T, L, O> {
        let RecordingCommand(command, _) = self;
        BeginCommand(command)
    }

    pub fn next_render_pass(self) -> Self {
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_next_subpass(
                L::buffer(&command.data),
                vk::SubpassContents::SECONDARY_COMMAND_BUFFERS,
            );
        }
        RecordingCommand(command, device)
    }

    pub fn write_secondary(self, secondary: &FinishedCommand<T, Secondary, O>) -> Self {
        let FinishedCommand(secondary) = secondary;
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_execute_commands(
                L::buffer(&command.data),
                &[Secondary::buffer(&secondary.data)],
            )
        }
        RecordingCommand(command, device)
    }

    pub fn copy_buffer<'b, 'c, S: MemoryProperties, D: MemoryProperties>(
        self,
        src: impl Into<&'b Buffer<S>>,
        dst: impl Into<&'c mut Buffer<D>>,
        ranges: &[vk::BufferCopy],
    ) -> Self {
        let RecordingCommand(command, device) = self;
        let src = src.into();
        let dst = dst.into();
        unsafe {
            device.cmd_copy_buffer(
                L::buffer(&command.data),
                src.get_vk_buffer(),
                dst.get_vk_buffer(),
                ranges,
            );
        }
        RecordingCommand(command, device)
    }

    pub fn change_layout<V: ImageType, M: MemoryProperties>(
        self,
        image: &mut Image<V, M>,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        array_layer: u32,
        base_level: u32,
        level_count: u32,
    ) -> Self {
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_pipeline_barrier(
                L::buffer(&command.data),
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::BY_REGION,
                &[],
                &[],
                &[vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::TRANSFER_READ
                        | vk::AccessFlags::TRANSFER_WRITE,
                    dst_access_mask: vk::AccessFlags::TRANSFER_READ
                        | vk::AccessFlags::TRANSFER_WRITE,
                    old_layout,
                    new_layout,
                    src_queue_family_index: O::get_queue_family_index(device),
                    dst_queue_family_index: O::get_queue_family_index(device),
                    image: image.get_vk_image(),
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: base_level,
                        level_count,
                        base_array_layer: array_layer,
                        layer_count: 1,
                    },
                    ..Default::default()
                }],
            );
        }
        RecordingCommand(command, device)
    }

    pub fn generate_mip<V: ImageType, M: MemoryProperties>(
        self,
        image: &mut Image<V, M>,
        array_layer: u32,
    ) -> Self {
        let image_info = image.get_image_info();
        let mip_info = image_info.mip_info.unwrap();
        let extent = vk::Extent2D {
            width: image_info.extent.width,
            height: image_info.extent.height,
        };
        (1..mip_info.level_count)
            .fold(self, |command, level| {
                command.generate_mip_level(image.get_vk_image(), extent, level, array_layer)
            })
            .change_layout(
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                array_layer,
                // TODO: Should it be mip_info.level_count + mip_info.mip_info.base_mip_level
                mip_info.level_count - 1,
                1,
            )
    }

    fn generate_mip_level(
        self,
        image: vk::Image,
        extent: vk::Extent2D,
        level: u32,
        layer: u32,
    ) -> Self {
        debug_assert!(level > 0, "generate mip level called for base mip level!");
        let base_level_extent = vk::Extent2D {
            width: (extent.width / 2u32.pow(level - 1)).max(1),
            height: (extent.height / 2u32.pow(level - 1)).max(1),
        };
        let level_extent = vk::Extent2D {
            width: (base_level_extent.width / 2).max(1),
            height: (base_level_extent.height / 2).max(1),
        };
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_pipeline_barrier(
                L::buffer(&command.data),
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::BY_REGION,
                &[],
                &[],
                &[vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    dst_access_mask: vk::AccessFlags::TRANSFER_READ,
                    old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    new_layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    src_queue_family_index: O::get_queue_family_index(device),
                    dst_queue_family_index: O::get_queue_family_index(device),
                    image,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: level - 1,
                        level_count: 1,
                        base_array_layer: layer,
                        layer_count: 1,
                    },
                    ..Default::default()
                }],
            );
            device.cmd_pipeline_barrier(
                L::buffer(&command.data),
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::BY_REGION,
                &[],
                &[],
                &[vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::TRANSFER_READ,
                    dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    old_layout: vk::ImageLayout::UNDEFINED,
                    new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    src_queue_family_index: O::get_queue_family_index(device),
                    dst_queue_family_index: O::get_queue_family_index(device),
                    image,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: level,
                        level_count: 1,
                        base_array_layer: layer,
                        layer_count: 1,
                    },
                    ..Default::default()
                }],
            );
            device.cmd_blit_image(
                L::buffer(&command.data),
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageBlit {
                    src_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: level - 1,
                        base_array_layer: layer,
                        layer_count: 1,
                    },
                    src_offsets: [
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D {
                            x: base_level_extent.width as i32,
                            y: base_level_extent.height as i32,
                            z: 1,
                        },
                    ],
                    dst_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: level,
                        base_array_layer: layer,
                        layer_count: 1,
                    },
                    dst_offsets: [
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D {
                            x: level_extent.width as i32,
                            y: level_extent.height as i32,
                            z: 1,
                        },
                    ],
                }],
                vk::Filter::LINEAR,
            );
        }
        RecordingCommand(command, device)
    }

    pub fn copy_image<V: ImageType, M: MemoryProperties, B: MemoryProperties>(
        self,
        src: &Buffer<B>,
        dst: &mut Image<V, M>,
        dst_layer: u32,
    ) -> Self {
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_copy_buffer_to_image(
                L::buffer(&command.data),
                src.get_vk_buffer(),
                dst.get_vk_image(),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: dst_layer,
                        layer_count: 1,
                    },
                    image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                    image_extent: dst.get_image_info().extent,
                }],
            );
        }
        RecordingCommand(command, device)
    }

    pub fn begin_render_pass<C: RenderPassConfig>(
        self,
        frame: &SwapchainFrame<C>,
        render_pass: &RenderPass<C>,
        clear_values: &Clear<C::Attachments>,
    ) -> Self {
        let RecordingCommand(command, device) = self;
        let clear_values = clear_values.get_clear_values();
        unsafe {
            device.cmd_begin_render_pass(
                L::buffer(&command.data),
                &vk::RenderPassBeginInfo {
                    render_pass: render_pass.handle,
                    framebuffer: frame.framebuffer.framebuffer,
                    render_area: frame.render_area,
                    clear_value_count: clear_values.len() as u32,
                    p_clear_values: clear_values.as_ptr(),
                    ..Default::default()
                },
                vk::SubpassContents::SECONDARY_COMMAND_BUFFERS,
            )
        }
        RecordingCommand(command, device)
    }

    pub fn end_render_pass(self) -> Self {
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_end_render_pass(L::buffer(&command.data));
        }
        RecordingCommand(command, device)
    }

    pub fn bind_pipeline(self, pipeline: impl Into<PipelineBindData>) -> Self {
        let binding = pipeline.into();
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_bind_pipeline(
                L::buffer(&command.data),
                binding.bind_point,
                binding.pipeline,
            );
        }
        RecordingCommand(command, device)
    }

    pub fn bind_vertex_buffer(self, buffer: impl Into<BufferBinding>) -> Self {
        let binding = buffer.into();
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_bind_vertex_buffers(
                L::buffer(&command.data),
                0,
                &[binding.buffer],
                &[binding.range.beg as vk::DeviceSize],
            );
        }
        RecordingCommand(command, device)
    }

    pub fn bind_index_buffer(
        self,
        buffer: impl Into<BufferBinding>,
        index_type: IndexType,
    ) -> Self {
        let binding = buffer.into();
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_bind_index_buffer(
                L::buffer(&command.data),
                binding.buffer,
                binding.range.beg as vk::DeviceSize,
                index_type.get_vk_index_type(),
            );
        }
        RecordingCommand(command, device)
    }

    pub fn push_constants<'b, P: PushConstant + Pod>(
        self,
        push_constant: impl Into<PushConstantDataRef<'b, P>>,
    ) -> Self {
        let push_constant = push_constant.into();
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_push_constants(
                L::buffer(&command.data),
                push_constant.layout,
                push_constant.range.stage_flags,
                push_constant.range.offset,
                bytes_of(push_constant.data),
            );
        }
        RecordingCommand(command, device)
    }

    pub fn bind_descriptor_set<'b>(self, descriptor: impl Into<&'b DescriptorBindingData>) -> Self {
        let binding = descriptor.into();
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_bind_descriptor_sets(
                L::buffer(&command.data),
                vk::PipelineBindPoint::GRAPHICS,
                binding.pipeline_layout,
                binding.set_index,
                &[binding.set],
                &[],
            )
        }
        RecordingCommand(command, device)
    }

    pub fn draw_indexed(self, draw: impl Into<DrawIndexed>) -> Self {
        let draw = draw.into();
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_draw_indexed(
                L::buffer(&command.data),
                draw.index_count,
                1,
                draw.index_offset,
                draw.vertex_offset,
                0,
            )
        }
        RecordingCommand(command, device)
    }
}
