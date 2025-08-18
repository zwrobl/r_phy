mod pool;
mod recording;

pub use pool::*;
pub use recording::*;

use ash::{self, vk};

use crate::{
    Device,
    error::ExtResult,
    resources::{
        framebuffer::FramebufferHandle,
        render_pass::{RenderPass, RenderPassConfig, Subpass},
    },
};

use std::{any::type_name, fmt::Debug, marker::PhantomData};

pub trait Lifetime: 'static + Debug {}

#[derive(Debug)]
pub struct Transient;

impl Lifetime for Transient {}

#[derive(Debug)]
pub struct Persistent;

impl Lifetime for Persistent {}

use crate::Context;

pub trait Operation: 'static + Debug {
    fn get_queue(device: &Device) -> vk::Queue;
    fn get_queue_family_index(device: &Device) -> u32;
    fn get_transient_command_pool(context: &Context) -> TransientCommandPool<Self>;
}

#[derive(Debug)]
pub struct Graphics;
#[derive(Debug)]
pub struct Transfer;
#[derive(Debug)]
pub struct Compute;

impl Operation for Graphics {
    fn get_queue(device: &Device) -> vk::Queue {
        device.device_queues.graphics
    }
    fn get_queue_family_index(device: &Device) -> u32 {
        device.physical_device.queue_families.graphics
    }
    fn get_transient_command_pool(context: &Context) -> TransientCommandPool<Self> {
        context
            .get_unique_resource::<TransientCommandPool<Self>, _>()
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
            .get_unique_resource::<TransientCommandPool<Self>, _>()
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
            .get_unique_resource::<TransientCommandPool<Self>, _>()
            .unwrap()
    }
}

pub struct Command<T: Lifetime, L: Level, O: Operation> {
    data: L::CommandData,
    _phantom: PhantomData<(T, O)>,
}

pub struct NewCommand<T: Lifetime, L: Level, O: Operation>(Command<T, L, O>);

impl<'a, T: Lifetime, L: Level, O: Operation> From<&'a NewCommand<T, L, O>>
    for &'a Command<T, L, O>
{
    fn from(value: &'a NewCommand<T, L, O>) -> Self {
        &value.0
    }
}

impl Device {
    pub fn begin_secondary_command<
        T: Lifetime,
        O: Operation,
        C: RenderPassConfig,
        S: Subpass<C::Attachments>,
    >(
        &self,
        command: NewCommand<T, Secondary, O>,
        render_pass: RenderPass<C>,
        framebuffer: FramebufferHandle<C>,
    ) -> ExtResult<BeginCommand<T, Secondary, O>> {
        let subpass = C::try_get_subpass_index::<S>().unwrap_or_else(|| {
            panic!(
                "Subpass {} not present in RenderPass {}!",
                type_name::<S>(),
                type_name::<C>(),
            )
        }) as u32;
        let NewCommand(command) = command;
        unsafe {
            self.begin_command_buffer(
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

    pub fn begin_primary_command<T: Lifetime, O: Operation>(
        &self,
        command: NewCommand<T, Primary, O>,
    ) -> ExtResult<BeginCommand<T, Primary, O>> {
        let NewCommand(command) = command;
        unsafe {
            self.wait_for_fences(&[command.data.fence], true, u64::MAX)?;
            self.reset_fences(&[command.data.fence])?;
            self.begin_command_buffer(
                command.data.buffer,
                &vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
        }
        Ok(BeginCommand(command))
    }

    pub fn finish_command<T: Lifetime, L: Level, O: Operation>(
        &self,
        command: BeginCommand<T, L, O>,
    ) -> ExtResult<FinishedCommand<T, L, O>> {
        let BeginCommand(command) = command;
        unsafe {
            self.end_command_buffer(L::buffer(&command.data))?;
        }
        Ok(FinishedCommand(command))
    }
}

pub struct BeginCommand<T: Lifetime, L: Level, O: Operation>(Command<T, L, O>);

impl<'a, T: Lifetime, L: Level, O: Operation> From<&'a BeginCommand<T, L, O>>
    for &'a Command<T, L, O>
{
    fn from(value: &'a BeginCommand<T, L, O>) -> Self {
        &value.0
    }
}

pub struct SubmitSemaphoreState<'a> {
    pub semaphores: &'a [vk::Semaphore],
    pub masks: &'a [vk::PipelineStageFlags],
}

pub struct FinishedCommand<T: Lifetime, L: Level, O: Operation>(Command<T, L, O>);

impl<'a, T: Lifetime, L: Level, O: Operation> From<&'a FinishedCommand<T, L, O>>
    for &'a Command<T, L, O>
{
    fn from(value: &'a FinishedCommand<T, L, O>) -> Self {
        &value.0
    }
}

impl Device {
    pub fn submit_command<'a, T: Lifetime, O: Operation>(
        &'a self,
        command: FinishedCommand<T, Primary, O>,
        wait: SubmitSemaphoreState,
        signal: &[vk::Semaphore],
    ) -> ExtResult<SubmitedCommand<'a, T, Primary, O>> {
        let FinishedCommand(command) = command;
        unsafe {
            self.queue_submit(
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
pub struct SubmitedCommand<'a, T: Lifetime, L: Level, O: Operation>(Command<T, L, O>, &'a Device);

impl<'a, T: Lifetime, L: Level, O: Operation> From<&'a SubmitedCommand<'a, T, L, O>>
    for &'a Command<T, L, O>
{
    fn from(value: &'a SubmitedCommand<T, L, O>) -> Self {
        &value.0
    }
}

impl<'a, O: Operation> SubmitedCommand<'a, Transient, Primary, O> {
    pub fn wait(self) -> ExtResult<Self> {
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

    pub fn _wait(self) -> ExtResult<Self> {
        let SubmitedCommand(command, device) = self;
        unsafe {
            device.wait_for_fences(&[command.data.fence], true, u64::MAX)?;
            device.reset_fences(&[command.data.fence])?;
        }
        Ok(Self(command, device))
    }
}
