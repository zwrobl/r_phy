use std::{
    convert::Infallible,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::copy_nonoverlapping,
};

use ash::vk;
use bytemuck::{cast_slice_mut, AnyBitPattern, NoUninit};
use type_kit::{Create, CreateResult, Destroy, DestroyResult, DropGuard, FromGuard, GuardVec};

use crate::{
    error::ExtResult,
    memory::{
        allocator::{AllocatorBuilder, AllocatorIndex},
        range::{ByteRange, Range},
        DeviceLocal, HostCoherent,
    },
    resources::{
        buffer::{
            BufferInfoBuilder, BufferPartial, BufferRaw, BufferUsage, PersistentBuffer, SharingMode,
        },
        command::{CopyBuffer, Graphics, Operation, SubmitSemaphoreState, Transfer},
        error::{GuardError, ResourceError},
        image::{Image, ImageType},
        Partial, Resource, ResourceGuardError,
    },
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

#[derive(Debug)]
pub struct StagingBufferPartial {
    partial: DropGuard<BufferPartial<HostCoherent>>,
}

impl Partial for StagingBufferPartial {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.partial.register_memory_requirements(builder);
    }
}

impl Create for StagingBufferPartial {
    type Config<'a> = StagingBufferBuilder;

    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let partial = BufferPartial::create(
            BufferInfoBuilder::<HostCoherent>::new()
                .with_usage(BufferUsage::TransferSrc)
                .with_sharing_mode(SharingMode::Exclusive)
                .with_queue_families(&[Transfer::get_queue_family_index(context)])
                .with_size(config.range.end),
            context,
        )?;
        Ok(StagingBufferPartial {
            partial: DropGuard::new(partial),
        })
    }
}

impl Destroy for StagingBufferPartial {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.partial.destroy(context);
        Ok(())
    }
}

#[derive(Debug)]
pub struct StagingBuffer {
    buffer: PersistentBuffer,
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
    ) -> ExtResult<()> {
        let command = context.allocate_transient_command::<Transfer>()?;
        let command = context.begin_primary_command(command)?;
        let command = context
            .start_recording(command)
            .push(&CopyBuffer::new(&self.buffer, dst.into()).push_range(
                0,
                dst_offset,
                self.buffer.get_size() as vk::DeviceSize,
            ))
            .stop_recording();
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
        context.free_transient_command(&command);
        Ok(())
    }

    pub fn transfer_image_data<V: ImageType>(
        &self,
        context: &Context,
        dst: &mut Image<V, DeviceLocal>,
        dst_array_layer: u32,
        dst_final_layout: vk::ImageLayout,
    ) -> ExtResult<()> {
        let image_info = dst.get_image_info();
        let mip_info = image_info.mip_info.unwrap_or_default();
        let old_layout = dst.get_vk_layout();
        let command =
            context.begin_primary_command(context.allocate_transient_command::<Graphics>()?)?;
        let command = context
            .start_recording(command)
            .push(
                &dst.change_layout(old_layout, vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .with_aspect(vk::ImageAspectFlags::COLOR)
                    .with_array_layer(dst_array_layer),
            )
            .push(
                &dst.copy_from_buffer(self)
                    .with_aspect(vk::ImageAspectFlags::COLOR)
                    .with_base_array_layer(dst_array_layer),
            )
            .push(&dst.generate_mip(dst_array_layer))
            .push(
                &dst.change_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL, dst_final_layout)
                    .with_aspect(vk::ImageAspectFlags::COLOR)
                    .with_array_layer(dst_array_layer)
                    .with_level_count(mip_info.level_count),
            )
            .stop_recording();

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
        context.free_transient_command(&command);
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

impl FromGuard for StagingBuffer {
    type Inner = BufferRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self.buffer.into_inner()
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        StagingBuffer {
            buffer: PersistentBuffer::from_inner(inner),
        }
    }
}

impl Create for StagingBuffer {
    type Config<'a> = (DropGuard<StagingBufferPartial>, Option<AllocatorIndex>);
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (buffer_partial, allocator) = config;
        let StagingBufferPartial { partial } = unsafe { buffer_partial.unwrap() };
        let buffer = PersistentBuffer::create((partial, allocator), context)?;
        Ok(StagingBuffer { buffer })
    }
}

impl Destroy for StagingBuffer {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.buffer.destroy(context);
        Ok(())
    }
}

impl Resource for StagingBuffer {
    type RawType = BufferRaw;
    type RawCollection = GuardVec<Self::RawType>;

    #[inline]
    fn wrap_guard_error((resource, err): ResourceGuardError<Self>) -> ResourceError {
        ResourceError::GuardError(GuardError::Buffer {
            error: (DropGuard::new(resource), err),
        })
    }
}
