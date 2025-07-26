use ash::{self, vk};
use bytemuck::{bytes_of, Pod};
use graphics::renderer::camera::CameraMatrices;
use math::types::Vector4;
use type_kit::{Create, CreateResult, Destroy, DestroyResult, FromGuard};

use crate::context::{
    device::{
        raw::resources::command::level::{PersistentAllocator, PersistentAllocatorRaw},
        raw::{
            resources::{
                buffer::Buffer,
                image::{Image, ImageType},
                pipeline::{GraphicsPipelineConfig, PipelineBindData, PushConstantDataRef},
                Resource,
            },
            unique::{
                layout::PushConstant,
                render_pass::{RenderPass, RenderPassConfig, Subpass},
                TypeUniqueResource,
            },
        },
    },
    error::{AshResult, ResourceError},
    Context,
};

use self::{
    level::{Level, Primary, Secondary},
    operation::Operation,
};

use crate::context::device::{
    framebuffer::{AttachmentList, Clear, FramebufferHandle},
    memory::MemoryProperties,
    raw::resources::descriptor::DescriptorBindingData,
    resources::{BufferType, LayoutSkybox, MeshPackBinding, MeshRangeBindData, Skybox},
    swapchain::SwapchainFrame,
    Device,
};
use std::{any::type_name, convert::Infallible, error::Error, marker::PhantomData};

pub struct Transient;
pub struct Persistent;

pub mod level {
    use std::{convert::Infallible, ptr::NonNull};

    use ash::vk;
    use type_kit::{Destroy, DestroyResult};

    use crate::context::{device::Device, error::AshResult, Context};

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

    pub trait Level: 'static {
        const LEVEL: vk::CommandBufferLevel;
        type CommandData;
        type Allocator;

        fn buffer(command: &Self::CommandData) -> vk::CommandBuffer;

        fn create_persistent_allocator(
            device: &Device,
            command_pool: vk::CommandPool,
            size: usize,
        ) -> AshResult<PersistentAllocator>;

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
            self.buffers
                .take()
                .map(|mut buffers| drop(unsafe { Box::from_raw(buffers.as_mut()) }));
            self.fences.take().map(|mut fences| {
                unsafe { Box::from_raw(fences.as_mut()) }
                    .iter()
                    .for_each(|&fence| unsafe {
                        context.destroy_fence(fence, None);
                    })
            });
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
        ) -> AshResult<PersistentAllocator> {
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
            self.buffers
                .take()
                .map(|mut buffers| drop(unsafe { Box::from_raw(buffers.as_mut()) }));
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
        ) -> AshResult<PersistentAllocator> {
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
}

pub mod operation {
    use ash::vk;

    use crate::context::{
        device::{raw::resources::command::TransientCommandPool, Device},
        Context,
    };

    #[derive(Debug)]
    pub struct Graphics;
    #[derive(Debug)]
    pub struct Transfer;
    #[derive(Debug)]
    pub struct Compute;

    // Lots of pub(in path) syntax in this module
    // some of it contents could be moved to separate module
    // placed higher in the source tree
    pub trait Operation: 'static {
        fn get_queue(device: &Device) -> vk::Queue;
        fn get_queue_family_index(device: &Device) -> u32;
        fn get_transient_command_pool(context: &Context) -> TransientCommandPool<Self>;
    }

    impl Operation for Graphics {
        fn get_queue(device: &Device) -> vk::Queue {
            device.device_queues.graphics
        }
        fn get_queue_family_index(device: &Device) -> u32 {
            device.physical_device.queue_families.graphics
        }
        fn get_transient_command_pool(context: &Context) -> TransientCommandPool<Self> {
            context
                .get_or_create_unique_resource::<TransientCommandPool<Self>, _>()
                .unwrap()
        }
    }
    impl Operation for Compute {
        fn get_queue(device: &Device) -> vk::Queue {
            device.device_queues.compute
        }
        fn get_queue_family_index(device: &Device) -> u32 {
            device.physical_device.queue_families.compute
        }
        fn get_transient_command_pool(context: &Context) -> TransientCommandPool<Self> {
            context
                .get_or_create_unique_resource::<TransientCommandPool<Self>, _>()
                .unwrap()
        }
    }
    impl Operation for Transfer {
        fn get_queue(device: &Device) -> vk::Queue {
            device.device_queues.transfer
        }
        fn get_queue_family_index(device: &Device) -> u32 {
            device.physical_device.queue_families.transfer
        }
        fn get_transient_command_pool(context: &Context) -> TransientCommandPool<Self> {
            context
                .get_or_create_unique_resource::<TransientCommandPool<Self>, _>()
                .unwrap()
        }
    }
}

pub struct Command<T, L: Level, O: Operation> {
    data: L::CommandData,
    _phantom: PhantomData<(T, O)>,
}

#[derive(Debug, Clone, Copy)]
pub struct PersistentCommandPoolRaw {
    command_pool: vk::CommandPool,
    allocator: PersistentAllocatorRaw,
}

impl<L: Level, O: Operation> Resource for PersistentCommandPool<L, O> {
    type RawType = PersistentCommandPoolRaw;
}

impl<L: Level, O: Operation> FromGuard for PersistentCommandPool<L, O> {
    type Inner = PersistentCommandPoolRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner {
            command_pool: self.command_pool,
            allocator: self.allocator.into(),
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            command_pool: inner.command_pool,
            allocator: inner.allocator.into(),
            _phantom: PhantomData,
        }
    }
}

impl Destroy for PersistentCommandPoolRaw {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.allocator.destroy(context);
        unsafe {
            context.destroy_command_pool(self.command_pool, None);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct PersistentCommandPool<L: Level, O: Operation> {
    command_pool: vk::CommandPool,
    allocator: PersistentAllocator,
    _phantom: PhantomData<(L, O)>,
}

impl<L: Level, O: Operation> PersistentCommandPool<L, O> {
    pub fn next(&mut self) -> (usize, NewCommand<Persistent, L, O>) {
        let (index, data) = L::allocate_persistent_command_buffer(&mut self.allocator);
        let command = Command {
            data,
            _phantom: PhantomData,
        };
        (index, NewCommand(command))
    }
}

impl<L: Level, O: Operation> Create for PersistentCommandPool<L, O> {
    type Config<'a> = usize;
    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let command_pool = unsafe {
            context.create_command_pool(
                &vk::CommandPoolCreateInfo::builder()
                    .queue_family_index(O::get_queue_family_index(context))
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )?
        };
        let allocator = L::create_persistent_allocator(context, command_pool, config)?;
        Ok(PersistentCommandPool {
            command_pool,
            allocator,
            _phantom: PhantomData,
        })
    }
}

impl<L: Level, O: Operation> Destroy for PersistentCommandPool<L, O> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        L::destory_persistent_alocator(context, &mut self.allocator);
        unsafe {
            context.destroy_command_pool(self.command_pool, None);
        }
        Ok(())
    }
}

pub struct NewCommand<T, L: Level, O: Operation>(Command<T, L, O>);

impl<'a, T, L: Level, O: Operation> From<&'a NewCommand<T, L, O>> for &'a Command<T, L, O> {
    fn from(value: &'a NewCommand<T, L, O>) -> Self {
        &value.0
    }
}

impl Device {
    pub fn begin_secondary_command<
        T,
        O: Operation,
        C: RenderPassConfig,
        S: Subpass<C::Attachments>,
    >(
        &self,
        command: NewCommand<T, Secondary, O>,
        render_pass: RenderPass<C>,
        framebuffer: FramebufferHandle<C::Attachments>,
    ) -> Result<BeginCommand<T, Secondary, O>, Box<dyn Error>> {
        let subpass = C::try_get_subpass_index::<S>().unwrap_or_else(|| {
            panic!(
                "Subpass {} not present in RenderPass {}!",
                type_name::<S>(),
                type_name::<C>(),
            )
        }) as u32;
        let NewCommand(command) = command;
        unsafe {
            self.device.begin_command_buffer(
                Secondary::buffer(&command.data),
                &vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE)
                    .inheritance_info(&vk::CommandBufferInheritanceInfo {
                        render_pass: render_pass.handle,
                        subpass,
                        framebuffer: framebuffer.framebuffer,
                        ..Default::default()
                    }),
            )?;
        }
        Ok(BeginCommand(command))
    }

    pub fn begin_primary_command<T, O: Operation>(
        &self,
        command: NewCommand<T, Primary, O>,
    ) -> AshResult<BeginCommand<T, Primary, O>> {
        let NewCommand(command) = command;
        unsafe {
            self.device
                .wait_for_fences(&[command.data.fence], true, u64::MAX)?;
            self.device.reset_fences(&[command.data.fence])?;
            self.device.begin_command_buffer(
                command.data.buffer,
                &vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }
        Ok(BeginCommand(command))
    }

    pub fn record_command<
        T,
        L: Level,
        O: Operation,
        F: FnOnce(RecordingCommand<T, L, O>) -> RecordingCommand<T, L, O>,
    >(
        &self,
        command: BeginCommand<T, L, O>,
        recorder: F,
    ) -> BeginCommand<T, L, O> {
        let BeginCommand(command) = command;
        let RecordingCommand(command, _) = recorder(RecordingCommand(command, self));
        BeginCommand(command)
    }

    pub fn finish_command<T, L: Level, O: Operation>(
        &self,
        command: BeginCommand<T, L, O>,
    ) -> AshResult<FinishedCommand<T, L, O>> {
        let BeginCommand(command) = command;
        unsafe {
            self.device.end_command_buffer(L::buffer(&command.data))?;
        }
        Ok(FinishedCommand(command))
    }
}

pub struct RecordingCommand<'a, T, L: Level, O: Operation>(Command<T, L, O>, &'a Device);

impl<'a, T, L: Level, O: Operation> From<&'a RecordingCommand<'a, T, L, O>>
    for &'a Command<T, L, O>
{
    fn from(value: &'a RecordingCommand<T, L, O>) -> Self {
        &value.0
    }
}

pub struct BeginCommand<T, L: Level, O: Operation>(Command<T, L, O>);

impl<'a, T, L: Level, O: Operation> From<&'a BeginCommand<T, L, O>> for &'a Command<T, L, O> {
    fn from(value: &'a BeginCommand<T, L, O>) -> Self {
        &value.0
    }
}

impl<'a, T, L: Level, O: Operation> RecordingCommand<'a, T, L, O> {
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

    pub fn copy_image<'b, 'c, V: ImageType, M: MemoryProperties, B: MemoryProperties>(
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

    pub fn begin_render_pass<A: AttachmentList, C: RenderPassConfig<Attachments = A>>(
        self,
        frame: &SwapchainFrame<A>,
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

    pub fn bind_mesh_pack(self, pack: impl Into<MeshPackBinding>) -> Self {
        let pack = pack.into();
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_bind_index_buffer(
                L::buffer(&command.data),
                pack.buffer,
                pack.buffer_ranges[BufferType::Index].beg as vk::DeviceSize,
                vk::IndexType::UINT32,
            );
            device.cmd_bind_vertex_buffers(
                L::buffer(&command.data),
                0,
                &[pack.buffer],
                &[pack.buffer_ranges[BufferType::Vertex].beg as vk::DeviceSize],
            );
        }
        RecordingCommand(command, device)
    }

    pub fn draw_skybox<C: GraphicsPipelineConfig<Layout = LayoutSkybox>>(
        self,
        skybox: &Skybox<C>,
        mut camera_matrices: CameraMatrices,
    ) -> Self {
        camera_matrices.view[3] = Vector4::w();
        self.bind_pipeline(&*skybox.pipeline)
            .bind_descriptor_set(
                &skybox
                    .descriptor
                    .get(0)
                    .get_binding_data(&skybox.pipeline)
                    .unwrap(),
            )
            .bind_mesh_pack(&*skybox.mesh_pack)
            .push_constants(skybox.pipeline.get_push_range(&camera_matrices))
            .draw_mesh(skybox.mesh_pack.get(0))
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

    pub fn draw_mesh(self, mesh: impl Into<MeshRangeBindData>) -> Self {
        let binding = mesh.into();
        let RecordingCommand(command, device) = self;
        unsafe {
            device.cmd_draw_indexed(
                L::buffer(&command.data),
                binding.index_count,
                1,
                binding.index_offset,
                binding.vertex_offset,
                0,
            )
        }
        RecordingCommand(command, device)
    }
}

pub struct SubmitSemaphoreState<'a> {
    pub semaphores: &'a [vk::Semaphore],
    pub masks: &'a [vk::PipelineStageFlags],
}

pub struct FinishedCommand<T, L: Level, O: Operation>(Command<T, L, O>);

impl<'a, T, L: Level, O: Operation> From<&'a FinishedCommand<T, L, O>> for &'a Command<T, L, O> {
    fn from(value: &'a FinishedCommand<T, L, O>) -> Self {
        &value.0
    }
}

impl Device {
    pub fn submit_command<'a, T, O: Operation>(
        &'a self,
        command: FinishedCommand<T, Primary, O>,
        wait: SubmitSemaphoreState,
        signal: &[vk::Semaphore],
    ) -> AshResult<SubmitedCommand<'a, T, Primary, O>> {
        let FinishedCommand(command) = command;
        unsafe {
            self.device.queue_submit(
                O::get_queue(self),
                &[vk::SubmitInfo {
                    command_buffer_count: 1,
                    p_command_buffers: [command.data.buffer].as_ptr(),
                    wait_semaphore_count: wait.semaphores.len() as _,
                    p_wait_semaphores: wait.semaphores.as_ptr(),
                    p_wait_dst_stage_mask: wait.masks.as_ptr(),
                    signal_semaphore_count: signal.len() as _,
                    p_signal_semaphores: signal.as_ptr(),
                    ..Default::default()
                }],
                command.data.fence,
            )?;
        }
        Ok(SubmitedCommand(command, self))
    }
}
pub struct SubmitedCommand<'a, T, L: Level, O: Operation>(Command<T, L, O>, &'a Device);

impl<'a, T, L: Level, O: Operation> From<&'a SubmitedCommand<'a, T, L, O>>
    for &'a Command<T, L, O>
{
    fn from(value: &'a SubmitedCommand<T, L, O>) -> Self {
        &value.0
    }
}

impl<'a, O: Operation> SubmitedCommand<'a, Transient, Primary, O> {
    pub fn wait(self) -> AshResult<Self> {
        let SubmitedCommand(command, device) = self;
        unsafe {
            device.wait_for_fences(&[command.data.fence], true, u64::MAX)?;
        }
        Ok(Self(command, device))
    }
}

impl<'a, O: Operation> SubmitedCommand<'a, Persistent, Primary, O> {
    pub fn _reset(self) -> NewCommand<Persistent, Primary, O> {
        let SubmitedCommand(command, _) = self;
        NewCommand(command)
    }

    pub fn _wait(self) -> Result<Self, Box<dyn Error>> {
        let SubmitedCommand(command, device) = self;
        unsafe {
            device.wait_for_fences(&[command.data.fence], true, u64::MAX)?;
            device.reset_fences(&[command.data.fence])?;
        }
        Ok(Self(command, device))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TransientCommandPoolRaw {
    pool: vk::CommandPool,
}

#[derive(Debug)]
pub struct TransientCommandPool<O: Operation + ?Sized> {
    pool: vk::CommandPool,
    _phantom: PhantomData<O>,
}

impl<O: Operation> TypeUniqueResource for TransientCommandPool<O> {
    type RawType = TransientCommandPoolRaw;
}

impl<O: Operation> FromGuard for TransientCommandPool<O> {
    type Inner = TransientCommandPoolRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner { pool: self.pool }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            pool: inner.pool,
            _phantom: PhantomData,
        }
    }
}

impl<O: Operation> Create for TransientCommandPool<O> {
    type Config<'a> = ();

    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(_config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let pool = unsafe {
            context.create_command_pool(
                &vk::CommandPoolCreateInfo::builder()
                    .queue_family_index(O::get_queue_family_index(&context))
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None,
            )?
        };
        Ok(Self {
            pool,
            _phantom: PhantomData,
        })
    }
}

impl<O: Operation> Destroy for TransientCommandPool<O> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe { context.destroy_command_pool(self.pool, None) };
        Ok(())
    }
}

impl Destroy for TransientCommandPoolRaw {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_command_pool(self.pool, None);
        }
        Ok(())
    }
}

impl Context {
    pub fn allocate_transient_command<O: Operation>(
        &self,
    ) -> AshResult<NewCommand<Transient, Primary, O>> {
        let pool = O::get_transient_command_pool(self);
        let &buffer = unsafe {
            self.device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::builder()
                        .level(Primary::LEVEL)
                        .command_pool(pool.pool)
                        .command_buffer_count(1),
                )?
                .first()
                .unwrap()
        };
        let fence = unsafe {
            self.device.create_fence(
                &vk::FenceCreateInfo {
                    flags: vk::FenceCreateFlags::SIGNALED,
                    ..Default::default()
                },
                None,
            )?
        };
        Ok(NewCommand(Command {
            data: Primary { buffer, fence },
            _phantom: PhantomData,
        }))
    }

    pub fn free_transient_command<'a, O: 'static + Operation>(
        &self,
        command: impl Into<&'a Command<Transient, Primary, O>>,
    ) {
        let &Command {
            data: Primary { buffer, fence },
            ..
        } = command.into();
        let pool = O::get_transient_command_pool(self);
        unsafe {
            self.device.free_command_buffers(pool.pool, &[buffer]);
            self.device.destroy_fence(fence, None);
        }
    }
}
