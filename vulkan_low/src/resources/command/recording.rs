use ash::vk;
use bytemuck::{bytes_of, Pod};

use crate::{
    device::Device,
    memory::{range::ByteRange, MemoryProperties},
    resources::{
        buffer::Buffer,
        command::{BeginCommand, Command, FinishedCommand, Level, Lifetime, Operation, Secondary},
        descriptor::{Descriptor, DescriptorBindingData},
        framebuffer::Clear,
        image::{Image, ImageType},
        layout::{DescriptorLayout, PushConstant},
        pipeline::{
            GraphicsPipeline, GraphicsPipelineConfig, PipelineBindData, PushConstantDataRef,
            PushConstantRangeMapper,
        },
        render_pass::{RenderPass, RenderPassConfig},
        swapchain::SwapchainFrame,
    },
};

pub trait Recorder {
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O>;
}

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

impl Device {
    pub fn start_recording<T: Lifetime, L: Level, O: Operation>(
        &self,
        command: BeginCommand<T, L, O>,
    ) -> RecordingCommand<T, L, O> {
        let BeginCommand(command) = command;
        RecordingCommand(command, self)
    }
}

pub struct BeginRenderPass<'a, C: RenderPassConfig> {
    frame: &'a SwapchainFrame<C>,
    clear_values: &'a Clear<C::Attachments>,
    render_pass: RenderPass<C>,
}

impl<C: RenderPassConfig> RenderPass<C> {
    #[inline]
    pub fn begin<'a>(
        &'a self,
        frame: &'a SwapchainFrame<C>,
        clear_values: &'a Clear<C::Attachments>,
    ) -> BeginRenderPass<'a, C> {
        BeginRenderPass {
            frame,
            clear_values,
            render_pass: *self,
        }
    }
}

impl<'a, C: RenderPassConfig> Recorder for BeginRenderPass<'a, C> {
    #[inline]
    fn record<'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'b, T, L, O>,
    ) -> RecordingCommand<'b, T, L, O> {
        let RecordingCommand(command, device) = command;
        let clear_values = self.clear_values.get_clear_values();
        unsafe {
            device.cmd_begin_render_pass(
                L::buffer(&command.data),
                &vk::RenderPassBeginInfo {
                    render_pass: self.render_pass.handle,
                    framebuffer: self.frame.framebuffer.framebuffer,
                    render_area: self.frame.render_area,
                    clear_value_count: clear_values.len() as u32,
                    p_clear_values: clear_values.as_ptr(),
                    ..Default::default()
                },
                vk::SubpassContents::SECONDARY_COMMAND_BUFFERS,
            )
        }
        RecordingCommand(command, device)
    }
}

pub struct NextRenderPass;

impl Recorder for NextRenderPass {
    #[inline]
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_next_subpass(
                L::buffer(&command.data),
                vk::SubpassContents::SECONDARY_COMMAND_BUFFERS,
            );
        }
        RecordingCommand(command, device)
    }
}

pub struct EndRenderPass;

impl Recorder for EndRenderPass {
    #[inline]
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_end_render_pass(L::buffer(&command.data));
        }
        RecordingCommand(command, device)
    }
}

impl<ET: Lifetime, EO: Operation> Recorder for FinishedCommand<ET, Secondary, EO> {
    #[inline]
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let FinishedCommand(secondary) = self;
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_execute_commands(
                L::buffer(&command.data),
                &[Secondary::buffer(&secondary.data)],
            )
        }
        RecordingCommand(command, device)
    }
}

pub struct BindPipeline {
    binding: PipelineBindData,
}

impl<C: GraphicsPipelineConfig> GraphicsPipeline<C> {
    #[inline]
    pub fn bind(&self) -> BindPipeline {
        BindPipeline {
            binding: self.get_binding_data(),
        }
    }
}

impl BindPipeline {
    #[inline]
    pub fn new(binding: PipelineBindData) -> Self {
        BindPipeline { binding }
    }
}

impl Recorder for BindPipeline {
    #[inline]
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_bind_pipeline(
                L::buffer(&command.data),
                self.binding.bind_point,
                self.binding.pipeline,
            );
        }
        RecordingCommand(command, device)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BindVertexBuffer {
    binding: BufferBinding,
}

impl<M: MemoryProperties> Buffer<M> {
    #[inline]
    pub fn bind_vertex(&self, range: ByteRange) -> BindVertexBuffer {
        BindVertexBuffer {
            binding: BufferBinding {
                buffer: self.get_vk_buffer(),
                range,
            },
        }
    }

    #[inline]
    pub fn bind_index(&self, range: ByteRange, index_type: IndexType) -> BindIndexBuffer {
        BindIndexBuffer {
            binding: BufferBinding {
                buffer: self.get_vk_buffer(),
                range,
            },
            index_type,
        }
    }
}

impl Recorder for BindVertexBuffer {
    #[inline]
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_bind_vertex_buffers(
                L::buffer(&command.data),
                0,
                &[self.binding.buffer],
                &[self.binding.range.beg as vk::DeviceSize],
            );
        }
        RecordingCommand(command, device)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BindIndexBuffer {
    binding: BufferBinding,
    index_type: IndexType,
}

impl Recorder for BindIndexBuffer {
    #[inline]
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_bind_index_buffer(
                L::buffer(&command.data),
                self.binding.buffer,
                self.binding.range.beg as vk::DeviceSize,
                self.index_type.get_vk_index_type(),
            );
        }
        RecordingCommand(command, device)
    }
}

pub struct PushConstants<'a, P: PushConstant + Pod> {
    data: PushConstantDataRef<'a, P>,
}

impl<C: GraphicsPipelineConfig> GraphicsPipeline<C> {
    #[inline]
    pub fn push_constants<'a, P: PushConstant + Pod>(
        &self,
        push_constant_data: &'a P,
    ) -> PushConstants<'a, P> {
        PushConstants {
            data: self.get_push_range(push_constant_data),
        }
    }
}

impl PushConstantRangeMapper {
    pub fn push_constants<'a, P: PushConstant + Pod>(
        &self,
        push_constant_data: &'a P,
    ) -> Option<PushConstants<'a, P>> {
        self.map_push_constant(push_constant_data)
            .map(|data| PushConstants { data })
    }
}

impl<'b, P: PushConstant + Pod> Recorder for PushConstants<'b, P> {
    #[inline]
    fn record<'a, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_push_constants(
                L::buffer(&command.data),
                self.data.layout,
                self.data.range.stage_flags,
                self.data.range.offset,
                bytes_of(self.data.data),
            );
        }
        RecordingCommand(command, device)
    }
}

pub struct BindDescriptor {
    binding: DescriptorBindingData,
}

impl<T: DescriptorLayout> Descriptor<T> {
    #[inline]
    pub fn bind<C: GraphicsPipelineConfig>(
        &self,
        pipeline: &GraphicsPipeline<C>,
    ) -> BindDescriptor {
        BindDescriptor {
            binding: self.get_binding_data(pipeline),
        }
    }
}

impl Recorder for BindDescriptor {
    #[inline]
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_bind_descriptor_sets(
                L::buffer(&command.data),
                // TODO: Needs to be changed if compute pipelines are used
                vk::PipelineBindPoint::GRAPHICS,
                self.binding.pipeline_layout,
                self.binding.set_index,
                // TODO: Could adapted used to bind multiple sets at once
                &[self.binding.set],
                &[],
            );
        }
        RecordingCommand(command, device)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DrawIndexed {
    pub index_count: u32,
    pub index_offset: u32,
    pub vertex_offset: i32,
}

impl Recorder for DrawIndexed {
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_draw_indexed(
                L::buffer(&command.data),
                self.index_count,
                1,
                self.index_offset,
                self.vertex_offset,
                0,
            )
        }
        RecordingCommand(command, device)
    }
}

pub struct CopyBuffer<'a, M1: MemoryProperties, M2: MemoryProperties> {
    src: &'a Buffer<M1>,
    dst: &'a Buffer<M2>,
    ranges: Vec<vk::BufferCopy>,
}

impl<'a, M1: MemoryProperties, M2: MemoryProperties> CopyBuffer<'a, M1, M2> {
    #[inline]
    pub fn new(src: &'a Buffer<M1>, dst: &'a Buffer<M2>) -> Self {
        CopyBuffer {
            src,
            dst,
            ranges: vec![],
        }
    }

    #[inline]
    pub fn push_range(
        mut self,
        src_offset: vk::DeviceSize,
        dst_offset: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> Self {
        self.ranges.push(vk::BufferCopy {
            src_offset,
            dst_offset,
            size,
        });
        self
    }
}

impl<M1: MemoryProperties, M2: MemoryProperties> Recorder for CopyBuffer<'_, M1, M2> {
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_copy_buffer(
                L::buffer(&command.data),
                self.src.get_vk_buffer(),
                self.dst.get_vk_buffer(),
                &self.ranges,
            );
        }
        RecordingCommand(command, device)
    }
}

pub struct CopyImageFromBuffer<'a, V: ImageType, M1: MemoryProperties, M2: MemoryProperties> {
    src: &'a Buffer<M1>,
    dst: &'a Image<V, M2>,
    aspect: vk::ImageAspectFlags,
    base_array_layer: u32,
    layer_count: u32,
    mip_level: u32,
}

impl<V: ImageType, M: MemoryProperties> Image<V, M> {
    #[inline]
    pub fn copy_from_buffer<'b, M1: MemoryProperties>(
        &'b self,
        src: &'b Buffer<M1>,
    ) -> CopyImageFromBuffer<'b, V, M1, M> {
        CopyImageFromBuffer {
            src,
            dst: self,
            aspect: vk::ImageAspectFlags::empty(),
            base_array_layer: 0,
            layer_count: 1,
            mip_level: 0,
        }
    }
}

impl<'a, V: ImageType, M1: MemoryProperties, M2: MemoryProperties>
    CopyImageFromBuffer<'a, V, M1, M2>
{
    #[inline]
    pub fn with_aspect(mut self, aspect: vk::ImageAspectFlags) -> Self {
        self.aspect |= aspect;
        self
    }

    #[inline]
    pub fn with_base_array_layer(mut self, base_array_layer: u32) -> Self {
        self.base_array_layer = base_array_layer;
        self
    }

    #[inline]
    pub fn with_layer_count(mut self, layer_count: u32) -> Self {
        self.layer_count = layer_count;
        self
    }

    #[inline]
    pub fn with_mip_level(mut self, mip_level: u32) -> Self {
        self.mip_level = mip_level;
        self
    }
}

impl<'b, V: ImageType, M1: MemoryProperties, M2: MemoryProperties> Recorder
    for CopyImageFromBuffer<'b, V, M1, M2>
{
    fn record<'a, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
        unsafe {
            device.cmd_copy_buffer_to_image(
                L::buffer(&command.data),
                self.src.get_vk_buffer(),
                self.dst.get_vk_image(),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: self.aspect,
                        mip_level: self.mip_level,
                        base_array_layer: self.base_array_layer,
                        layer_count: self.layer_count,
                    },
                    image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                    image_extent: self.dst.get_image_info().extent,
                }],
            );
        }
        RecordingCommand(command, device)
    }
}

pub struct ChangeImageLayout<'a, V: ImageType, M: MemoryProperties> {
    image: &'a Image<V, M>,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    array_layer: u32,
    layer_count: u32,
    base_level: u32,
    level_count: u32,
    aspect: vk::ImageAspectFlags,
}

impl<V: ImageType, M: MemoryProperties> Image<V, M> {
    #[inline]
    pub fn change_layout(
        &self,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
    ) -> ChangeImageLayout<V, M> {
        ChangeImageLayout {
            image: self,
            old_layout,
            new_layout,
            array_layer: 0,
            layer_count: 1,
            base_level: 0,
            level_count: 1,
            aspect: vk::ImageAspectFlags::empty(),
        }
    }
}

impl<'a, V: ImageType, M: MemoryProperties> ChangeImageLayout<'a, V, M> {
    #[inline]
    pub fn with_array_layer(mut self, array_layer: u32) -> Self {
        self.array_layer = array_layer;
        self
    }

    #[inline]
    pub fn with_layer_count(mut self, layer_count: u32) -> Self {
        self.layer_count = layer_count;
        self
    }

    #[inline]
    pub fn with_base_level(mut self, base_level: u32) -> Self {
        self.base_level = base_level;
        self
    }

    #[inline]
    pub fn with_level_count(mut self, level_count: u32) -> Self {
        self.level_count = level_count;
        self
    }

    #[inline]
    pub fn with_aspect(mut self, aspect: vk::ImageAspectFlags) -> Self {
        self.aspect |= aspect;
        self
    }
}

impl<V: ImageType, M: MemoryProperties> Recorder for ChangeImageLayout<'_, V, M> {
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
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
                    old_layout: self.old_layout,
                    new_layout: self.new_layout,
                    src_queue_family_index: O::get_queue_family_index(device),
                    dst_queue_family_index: O::get_queue_family_index(device),
                    image: self.image.get_vk_image(),
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: self.aspect,
                        base_mip_level: self.base_level,
                        level_count: self.level_count,
                        base_array_layer: self.array_layer,
                        layer_count: self.layer_count,
                    },
                    ..Default::default()
                }],
            );
        }
        RecordingCommand(command, device)
    }
}

struct GenerateMipLevel {
    image: vk::Image,
    base_level_extent: vk::Extent2D,
    level_extent: vk::Extent2D,
    level: u32,
    layer: u32,
}

impl GenerateMipLevel {
    pub fn new(image: vk::Image, extent: vk::Extent2D, level: u32, layer: u32) -> Self {
        let base_level_extent = vk::Extent2D {
            width: (extent.width / 2u32.pow(level - 1)).max(1),
            height: (extent.height / 2u32.pow(level - 1)).max(1),
        };
        let level_extent = vk::Extent2D {
            width: (base_level_extent.width / 2).max(1),
            height: (base_level_extent.height / 2).max(1),
        };
        GenerateMipLevel {
            image,
            base_level_extent,
            level_extent,
            level,
            layer,
        }
    }
}

impl Recorder for GenerateMipLevel {
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let RecordingCommand(command, device) = command;
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
                    image: self.image,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: self.level - 1,
                        level_count: 1,
                        base_array_layer: self.layer,
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
                    image: self.image,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: self.level,
                        level_count: 1,
                        base_array_layer: self.layer,
                        layer_count: 1,
                    },
                    ..Default::default()
                }],
            );
            device.cmd_blit_image(
                L::buffer(&command.data),
                self.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageBlit {
                    src_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: self.level - 1,
                        base_array_layer: self.layer,
                        layer_count: 1,
                    },
                    src_offsets: [
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D {
                            x: self.base_level_extent.width as i32,
                            y: self.base_level_extent.height as i32,
                            z: 1,
                        },
                    ],
                    dst_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: self.level,
                        base_array_layer: self.layer,
                        layer_count: 1,
                    },
                    dst_offsets: [
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D {
                            x: self.level_extent.width as i32,
                            y: self.level_extent.height as i32,
                            z: 1,
                        },
                    ],
                }],
                vk::Filter::LINEAR,
            );
        }
        RecordingCommand(command, device)
    }
}

pub struct GenerateMip<'a, V: ImageType, M: MemoryProperties> {
    image: &'a mut Image<V, M>,
    array_layer: u32,
}

impl<V: ImageType, M: MemoryProperties> Image<V, M> {
    #[inline]
    pub fn generate_mip(&mut self, array_layer: u32) -> GenerateMip<V, M> {
        GenerateMip {
            image: self,
            array_layer,
        }
    }
}

impl<'b, V: ImageType, M: MemoryProperties> Recorder for GenerateMip<'b, V, M> {
    fn record<'a, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        let image_info = self.image.get_image_info();
        let mip_info = image_info.mip_info.unwrap();
        let extent = vk::Extent2D {
            width: image_info.extent.width,
            height: image_info.extent.height,
        };
        let mip_levels = (1..mip_info.level_count)
            .map(|level| {
                GenerateMipLevel::new(self.image.get_vk_image(), extent, level, self.array_layer)
            })
            .collect::<Vec<_>>();
        command.extend(&mip_levels).push(
            &self
                .image
                .change_layout(
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                )
                .with_aspect(vk::ImageAspectFlags::COLOR)
                .with_array_layer(self.array_layer)
                // TODO: Should it be mip_info.level_count + mip_info.mip_info.base_mip_level
                .with_base_level(mip_info.level_count - 1)
                .with_level_count(1),
        )
    }
}

impl<'a, T: Lifetime, L: Level, O: Operation> RecordingCommand<'a, T, L, O> {
    #[inline]
    pub fn push<P: Recorder>(self, operation: &P) -> Self {
        operation.record(self)
    }

    #[inline]
    pub fn extend<P: Recorder>(self, operation: &[P]) -> Self {
        operation.iter().fold(self, |command, op| command.push(op))
    }

    pub fn stop_recording(self) -> BeginCommand<T, L, O> {
        let RecordingCommand(command, _) = self;
        BeginCommand(command)
    }
}
