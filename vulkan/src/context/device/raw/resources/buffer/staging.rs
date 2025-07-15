use std::{
    borrow::BorrowMut,
    convert::Infallible,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::copy_nonoverlapping,
};

use ash::vk;
use bytemuck::{cast_slice_mut, AnyBitPattern, NoUninit};
use type_kit::{Create, CreateResult, Destroy, DestroyResult};

use crate::context::{
    device::{
        command::{
            operation::{self, Operation},
            SubmitSemaphoreState,
        },
        memory::{DeviceLocal, HostCoherent},
        raw::{
            allocator::AllocatorIndex,
            range::{ByteRange, Range},
            resources::buffer::BufferInfoBuilder,
        },
        resources::image::Image2D,
        Device,
    },
    error::{VkError, VkResult},
    Context,
};

use super::Buffer;

pub struct StagingBufferBuilder {
    range: ByteRange,
}

impl Default for StagingBufferBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl StagingBufferBuilder {
    pub fn new() -> Self {
        Self {
            range: ByteRange::empty(),
        }
    }

    pub fn append<T: AnyBitPattern>(&mut self, len: usize) -> Range<T> {
        self.range.extend::<T>(len).into()
    }
}

pub struct StagingBuffer {
    buffer: Buffer<HostCoherent>,
}

pub struct WritableRange<T: AnyBitPattern> {
    ptr: *mut T,
    range: Range<T>,
}

impl<'a> From<&'a StagingBuffer> for &'a Buffer<HostCoherent> {
    fn from(value: &'a StagingBuffer) -> Self {
        &value.buffer
    }
}

impl<'a> From<&'a mut StagingBuffer> for &'a mut Buffer<HostCoherent> {
    fn from(value: &'a mut StagingBuffer) -> Self {
        &mut value.buffer
    }
}

impl Deref for StagingBuffer {
    type Target = Buffer<HostCoherent>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for StagingBuffer {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl StagingBuffer {
    pub fn transfer_buffer_data<'b>(
        &self,
        context: &Context,
        dst: impl Into<&'b mut Buffer<DeviceLocal>>,
        dst_offset: vk::DeviceSize,
    ) -> VkResult<()> {
        let command = context.allocate_transient_command::<operation::Transfer>()?;
        let command = context.begin_primary_command(command)?;
        let command = context.record_command(command, |command| {
            command.copy_buffer(
                &self.buffer,
                dst,
                &[vk::BufferCopy {
                    src_offset: 0,
                    dst_offset,
                    size: self.buffer.get_size() as vk::DeviceSize,
                }],
            )
        });
        let command = context
            .submit_command(
                context.finish_command(command)?,
                SubmitSemaphoreState {
                    semaphores: &[],
                    masks: &[],
                },
                &[],
            )?
            .wait()?;
        context.free_command(&command);
        Ok(())
    }

    pub fn transfer_image_data<'b>(
        &self,
        context: &Device,
        dst: impl Into<&'b mut Image2D<DeviceLocal>>,
        dst_array_layer: u32,
        dst_final_layout: vk::ImageLayout,
    ) -> VkResult<()> {
        let dst: &mut _ = dst.into();
        debug_assert!(
            dst.array_layers > dst_array_layer,
            "Invalid dst_array_layer for image data transfer!"
        );
        let dst_mip_levels = dst.mip_levels;
        let dst_old_layout = dst.layout;
        let command = context
            .begin_primary_command(context.allocate_transient_command::<operation::Graphics>()?)?;
        let command = context.record_command(command, |command| {
            command
                .change_layout(
                    dst.borrow_mut(),
                    dst_old_layout,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    dst_array_layer,
                    0,
                    1,
                )
                .copy_image(self, dst.borrow_mut(), dst_array_layer)
                .generate_mip(dst.borrow_mut(), dst_array_layer)
                .change_layout(
                    dst.borrow_mut(),
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    dst_final_layout,
                    dst_array_layer,
                    0,
                    dst_mip_levels,
                )
        });

        let command = context
            .submit_command(
                context.finish_command(command)?,
                SubmitSemaphoreState {
                    semaphores: &[],
                    masks: &[],
                },
                &[],
            )?
            .wait()?;
        context.free_command(&command);
        Ok(())
    }

    pub fn write_range<T: AnyBitPattern>(&mut self, range: Range<T>) -> WritableRange<T> {
        debug_assert!(
            <Range<T> as Into<ByteRange>>::into(range).end <= self.buffer.get_size(),
            "Invalid range for StagingBuffer write!"
        );
        WritableRange {
            range: Range {
                first: 0,
                len: range.len,
                _phantom: PhantomData,
            },
            ptr: unsafe { (self.buffer.ptr.unwrap() as *mut T).add(range.first) },
        }
    }
}

impl<T: AnyBitPattern> WritableRange<T> {
    pub fn write(&mut self, value: &[T]) -> Range<T> {
        let range = self.range.alloc(value.len());
        unsafe { copy_nonoverlapping(value.as_ptr(), self.ptr.add(range.first), value.len()) }
        range
    }
}

impl<T: AnyBitPattern + NoUninit> WritableRange<T> {
    pub fn remaining_as_slice_mut(&mut self) -> &mut [T] {
        let range = self.range.alloc(self.range.len);
        let values =
            unsafe { std::slice::from_raw_parts_mut::<T>(self.ptr.add(range.first), range.len) };
        cast_slice_mut(values)
    }
}

impl Create for StagingBuffer {
    type Config<'a> = (StagingBufferBuilder, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (StagingBufferBuilder { range }, allocator) = config;
        let buffer = Buffer::create(
            (
                BufferInfoBuilder::<HostCoherent>::new()
                    .with_usage(vk::BufferUsageFlags::TRANSFER_SRC)
                    .with_sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .with_queue_families(&[operation::Transfer::get_queue_family_index(context)])
                    .with_size(range.end as vk::DeviceSize)
                    .build(),
                allocator,
            ),
            context,
        )?;
        Ok(StagingBuffer { buffer })
    }
}

impl Destroy for StagingBuffer {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.buffer.destroy(context);
        Ok(())
    }
}
