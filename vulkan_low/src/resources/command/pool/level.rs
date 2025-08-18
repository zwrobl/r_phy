use std::{convert::Infallible, fmt::Debug, ptr::NonNull};

use ash::vk;
use type_kit::{Destroy, DestroyResult};

use crate::{Context, device::Device, error::ExtResult};

#[derive(Debug, Clone, Copy)]
pub enum PersistentAllocatorRaw {
    Primary(PrimaryPersistenAllocatorRaw),
    Secondary(SecondaryPersistentAllocatorRaw),
}

impl Destroy for PersistentAllocatorRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        match self {
            Self::Primary(allocator) => allocator.destroy(context),
            Self::Secondary(allocator) => allocator.destroy(()),
        }
    }
}

impl From<PersistentAllocator> for PersistentAllocatorRaw {
    #[inline]
    fn from(value: PersistentAllocator) -> Self {
        match value {
            PersistentAllocator::Primary(allocator) => Self::Primary(allocator.into()),
            PersistentAllocator::Secondary(allocator) => Self::Secondary(allocator.into()),
        }
    }
}

impl From<PersistentAllocatorRaw> for PersistentAllocator {
    #[inline]
    fn from(value: PersistentAllocatorRaw) -> Self {
        match value {
            PersistentAllocatorRaw::Primary(allocator) => Self::Primary(allocator.into()),
            PersistentAllocatorRaw::Secondary(allocator) => Self::Secondary(allocator.into()),
        }
    }
}

#[derive(Debug)]
pub enum PersistentAllocator {
    Primary(PrimaryPersistenAllocator),
    Secondary(SecondaryPersistentAllocator),
}

pub trait Level: 'static + Debug {
    const LEVEL: vk::CommandBufferLevel;
    type CommandData;
    type Allocator;

    fn buffer(command: &Self::CommandData) -> vk::CommandBuffer;

    fn create_persistent_allocator(
        device: &Device,
        command_pool: vk::CommandPool,
        size: usize,
    ) -> ExtResult<PersistentAllocator>;

    fn destory_persistent_alocator(device: &Device, allocator: &mut PersistentAllocator);

    fn allocate_persistent_command_buffer(
        allocator: &mut PersistentAllocator,
    ) -> (usize, Self::CommandData);
}

#[derive(Debug, Clone, Copy)]
pub struct PrimaryPersistenAllocatorRaw {
    index: usize,
    buffers: Option<NonNull<[vk::CommandBuffer]>>,
    fences: Option<NonNull<[vk::Fence]>>,
}

impl Destroy for PrimaryPersistenAllocatorRaw {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        if let Some(mut buffers) = self.buffers.take() {
            drop(unsafe { Box::from_raw(buffers.as_mut()) });
        }
        if let Some(mut fences) = self.fences.take() {
            unsafe { Box::from_raw(fences.as_mut()) }
                .iter()
                .for_each(|&fence| unsafe {
                    context.destroy_fence(fence, None);
                })
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct PrimaryPersistenAllocator {
    index: usize,
    buffers: Box<[vk::CommandBuffer]>,
    fences: Box<[vk::Fence]>,
}

impl From<PrimaryPersistenAllocatorRaw> for PrimaryPersistenAllocator {
    #[inline]
    fn from(mut value: PrimaryPersistenAllocatorRaw) -> Self {
        Self {
            index: value.index,
            buffers: unsafe { Box::from_raw(value.buffers.take().unwrap().as_mut()) },
            fences: unsafe { Box::from_raw(value.fences.take().unwrap().as_mut()) },
        }
    }
}

impl From<PrimaryPersistenAllocator> for PrimaryPersistenAllocatorRaw {
    #[inline]
    fn from(value: PrimaryPersistenAllocator) -> Self {
        Self {
            index: value.index,
            buffers: NonNull::new(Box::leak(value.buffers)),
            fences: NonNull::new(Box::leak(value.fences)),
        }
    }
}

impl<'a> TryFrom<&'a PersistentAllocator> for &'a PrimaryPersistenAllocator {
    type Error = ();

    #[inline]
    fn try_from(value: &'a PersistentAllocator) -> Result<Self, Self::Error> {
        match value {
            PersistentAllocator::Primary(allocator) => Ok(allocator),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<&'a mut PersistentAllocator> for &'a mut PrimaryPersistenAllocator {
    type Error = ();

    #[inline]
    fn try_from(value: &'a mut PersistentAllocator) -> Result<Self, Self::Error> {
        match value {
            PersistentAllocator::Primary(allocator) => Ok(allocator),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub struct Primary {
    pub buffer: vk::CommandBuffer,
    pub fence: vk::Fence,
}

impl Level for Primary {
    const LEVEL: vk::CommandBufferLevel = vk::CommandBufferLevel::PRIMARY;
    type CommandData = Self;
    type Allocator = PrimaryPersistenAllocator;

    fn allocate_persistent_command_buffer(
        allocator: &mut PersistentAllocator,
    ) -> (usize, Self::CommandData) {
        let allocator: &mut Self::Allocator = allocator.try_into().unwrap();
        let index = allocator.index;
        allocator.index = (allocator.index + 1) % allocator.buffers.len();
        (
            index,
            Self {
                buffer: allocator.buffers[index],
                fence: allocator.fences[index],
            },
        )
    }

    fn create_persistent_allocator(
        device: &Device,
        command_pool: vk::CommandPool,
        size: usize,
    ) -> ExtResult<PersistentAllocator> {
        let allocate_info = vk::CommandBufferAllocateInfo {
            command_pool,
            level: Self::LEVEL,
            command_buffer_count: size as u32,
            ..Default::default()
        };
        let (buffers, fences) = unsafe {
            let buffers = device.allocate_command_buffers(&allocate_info)?;
            let fences = (0..buffers.len())
                .map(|_| {
                    device.create_fence(
                        &vk::FenceCreateInfo {
                            flags: vk::FenceCreateFlags::SIGNALED,
                            ..Default::default()
                        },
                        None,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            (buffers, fences)
        };
        Ok(PersistentAllocator::Primary(PrimaryPersistenAllocator {
            buffers: buffers.into_boxed_slice(),
            fences: fences.into_boxed_slice(),
            index: 0,
        }))
    }

    fn destory_persistent_alocator(device: &Device, allocator: &mut PersistentAllocator) {
        let allocator: &mut Self::Allocator = allocator.try_into().unwrap();
        unsafe {
            allocator
                .fences
                .iter()
                .for_each(|&fence| device.destroy_fence(fence, None));
        }
    }

    fn buffer(command: &Self::CommandData) -> vk::CommandBuffer {
        command.buffer
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SecondaryPersistentAllocatorRaw {
    index: usize,
    buffers: Option<NonNull<[vk::CommandBuffer]>>,
}

impl Destroy for SecondaryPersistentAllocatorRaw {
    type Context<'a> = ();

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> DestroyResult<Self> {
        if let Some(mut buffers) = self.buffers.take() {
            drop(unsafe { Box::from_raw(buffers.as_mut()) });
        }
        Ok(())
    }
}

impl From<SecondaryPersistentAllocatorRaw> for SecondaryPersistentAllocator {
    #[inline]
    fn from(mut value: SecondaryPersistentAllocatorRaw) -> Self {
        Self {
            index: value.index,
            buffers: unsafe { Box::from_raw(value.buffers.take().unwrap().as_mut()) },
        }
    }
}

impl From<SecondaryPersistentAllocator> for SecondaryPersistentAllocatorRaw {
    #[inline]
    fn from(value: SecondaryPersistentAllocator) -> Self {
        Self {
            index: value.index,
            buffers: NonNull::new(Box::leak(value.buffers)),
        }
    }
}

#[derive(Debug)]
pub struct SecondaryPersistentAllocator {
    index: usize,
    buffers: Box<[vk::CommandBuffer]>,
}

impl<'a> TryFrom<&'a PersistentAllocator> for &'a SecondaryPersistentAllocator {
    type Error = ();

    #[inline]
    fn try_from(value: &'a PersistentAllocator) -> Result<Self, Self::Error> {
        match value {
            PersistentAllocator::Secondary(allocator) => Ok(allocator),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<&'a mut PersistentAllocator> for &'a mut SecondaryPersistentAllocator {
    type Error = ();

    #[inline]
    fn try_from(value: &'a mut PersistentAllocator) -> Result<Self, Self::Error> {
        match value {
            PersistentAllocator::Secondary(allocator) => Ok(allocator),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub struct Secondary {
    pub buffer: vk::CommandBuffer,
}

impl Level for Secondary {
    const LEVEL: vk::CommandBufferLevel = vk::CommandBufferLevel::SECONDARY;
    type CommandData = Self;
    type Allocator = SecondaryPersistentAllocator;

    fn allocate_persistent_command_buffer(
        allocator: &mut PersistentAllocator,
    ) -> (usize, Self::CommandData) {
        let allocator: &mut Self::Allocator = allocator.try_into().unwrap();
        let index = allocator.index;
        allocator.index = (allocator.index + 1) % allocator.buffers.len();
        (
            index,
            Self {
                buffer: allocator.buffers[index],
            },
        )
    }

    fn create_persistent_allocator(
        device: &Device,
        command_pool: vk::CommandPool,
        size: usize,
    ) -> ExtResult<PersistentAllocator> {
        let allocate_info = vk::CommandBufferAllocateInfo {
            command_pool,
            level: Self::LEVEL,
            command_buffer_count: size as u32,
            ..Default::default()
        };
        let buffers = unsafe { device.allocate_command_buffers(&allocate_info)? };
        Ok(PersistentAllocator::Secondary(
            SecondaryPersistentAllocator {
                buffers: buffers.into_boxed_slice(),
                index: 0,
            },
        ))
    }

    fn destory_persistent_alocator(_device: &Device, _allocator: &mut PersistentAllocator) {
        // Buffers are destroyed with the command pool
    }

    fn buffer(command: &Self::CommandData) -> vk::CommandBuffer {
        command.buffer
    }
}
