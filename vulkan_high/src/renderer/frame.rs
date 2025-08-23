use std::{
    convert::Infallible,
    ops::{Deref, DerefMut},
};

use type_kit::{
    Cons, Create, CreateCollection, CreateResult, Destroy, DestroyCollection, DestroyResult,
    DropGuard, unpack_list,
};

use graphics::renderer::camera::CameraMatrices;
use vulkan_low::{
    Context, index_list,
    memory::allocator::{AllocatorBuilder, AllocatorIndex},
    resources::{
        Partial, ResourceIndex,
        buffer::{UniformBuffer, UniformBufferInfoBuilder, UniformBufferPartial},
        command::{
            BeginCommand, Graphics, Persistent, PersistentCommandPool, Primary, RecordingCommand,
        },
        descriptor::{Descriptor, DescriptorPool, DescriptorSetWriter},
        error::{ResourceError, ResourceResult},
        layout::presets::CameraDescriptorSet,
        render_pass::RenderPassConfig,
        storage::ResourceIndexListBuilder,
        swapchain::{
            Swapchain, SwapchainFrame, SwapchainFramebufferConfigBuilder, SwapchainPartial,
        },
        sync::Semaphore,
    },
};

#[derive(Debug)]
pub struct CameraUniformPartial {
    buffer: DropGuard<UniformBufferPartial<CameraMatrices, Graphics>>,
}

impl Destroy for CameraUniformPartial {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.buffer.destroy(context);
        Ok(())
    }
}

impl Partial for CameraUniformPartial {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.buffer.register_memory_requirements(builder);
    }
}

impl Create for CameraUniformPartial {
    type Config<'a> = usize;
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let buffer = UniformBufferPartial::create(
            UniformBufferInfoBuilder::new().with_len(config),
            context,
        )?;
        Ok(CameraUniformPartial {
            buffer: DropGuard::new(buffer),
        })
    }
}

#[derive(Debug)]
pub struct CameraUniform {
    pub descriptor_pool: ResourceIndex<DescriptorPool<CameraDescriptorSet>>,
    pub uniform_buffer: ResourceIndex<UniformBuffer<CameraMatrices, Graphics>>,
}

impl Create for CameraUniform {
    type Config<'a> = (DropGuard<CameraUniformPartial>, Option<AllocatorIndex>);
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (partial, allocator) = config;
        let CameraUniformPartial { buffer } = unsafe { partial.unwrap() };
        let uniform_buffer =
            context.create_resource::<UniformBuffer<_, _>, _>((buffer, allocator))?;
        let descriptor_pool = context.operate_ref(
            index_list![uniform_buffer],
            |unpack_list![uniform_buffer]| {
                let len = uniform_buffer.size();
                let descriptors = context.create_resource(
                    DescriptorSetWriter::<CameraDescriptorSet>::new(len)
                        .write_buffer(uniform_buffer),
                )?;
                Result::<_, ResourceError>::Ok(descriptors)
            },
        )??;
        Ok(CameraUniform {
            descriptor_pool,
            uniform_buffer,
        })
    }
}

impl Destroy for CameraUniform {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = context.destroy_resource(self.descriptor_pool);
        let _ = context.destroy_resource(self.uniform_buffer);
        Ok(())
    }
}

pub struct FramePoolPartial<C: RenderPassConfig> {
    camera_partial: DropGuard<CameraUniformPartial>,
    swapchain_partial: SwapchainPartial<C>,
}

impl<C: RenderPassConfig> Partial for FramePoolPartial<C> {
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.camera_partial.register_memory_requirements(builder);
    }
}

impl<C: RenderPassConfig> Create for FramePoolPartial<C> {
    type Config<'a> = ();
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(_config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let swapchain_partial = SwapchainPartial::create((), context)?;
        let camera_partial = DropGuard::new(CameraUniformPartial::create(
            swapchain_partial.num_images(),
            context,
        )?);
        Ok(FramePoolPartial {
            camera_partial,
            swapchain_partial,
        })
    }
}

impl<C: RenderPassConfig> Destroy for FramePoolPartial<C> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.camera_partial.destroy(context);
        let _ = self.swapchain_partial.destroy(context);
        Ok(())
    }
}

#[derive(Debug)]
pub struct FramePool<C: RenderPassConfig> {
    semaphores: Box<[Semaphore]>,
    camera_uniform: CameraUniform,
    command_pool: ResourceIndex<PersistentCommandPool<Primary, Graphics>>,
    pub swapchain: ResourceIndex<Swapchain<C>>,
}

impl<C: RenderPassConfig> FramePool<C> {
    #[inline]
    pub fn num_images(&self) -> usize {
        self.semaphores.len()
    }
}

impl<C: RenderPassConfig> Create for FramePool<C> {
    type Config<'a> = (
        &'a dyn SwapchainFramebufferConfigBuilder<C>,
        DropGuard<FramePoolPartial<C>>,
        Option<AllocatorIndex>,
    );
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (framebuffer_builder, partial, allocator) = config;
        let FramePoolPartial {
            camera_partial,
            swapchain_partial,
        } = unsafe { partial.unwrap() };
        let num_images = swapchain_partial.num_images();
        let swapchain = context.create_resource((framebuffer_builder, swapchain_partial))?;
        let camera_uniform = CameraUniform::create((camera_partial, allocator), context)?;
        let semaphores = (0..num_images)
            .map(|_| ())
            .create(context)
            .collect::<Result<Box<_>, _>>()?;
        let command_pool = context.create_resource(num_images)?;
        Ok(FramePool {
            semaphores,
            camera_uniform,
            command_pool,
            swapchain,
        })
    }
}

impl<C: RenderPassConfig> Destroy for FramePool<C> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.semaphores.iter_mut().destroy(context)?;
        self.camera_uniform.destroy(context)?;
        let _ = context.destroy_resource(self.command_pool);
        let _ = context.destroy_resource(self.swapchain);
        Ok(())
    }
}

pub struct Frame<C: RenderPassConfig> {
    pub swapchain_frame: SwapchainFrame<C>,
    pub camera_matrices: CameraMatrices,
    pub camera_descriptor: Descriptor<CameraDescriptorSet>,
    pub primary_command: Option<BeginCommand<Persistent, Primary, Graphics>>,
}

impl<C: RenderPassConfig> Frame<C> {
    #[inline]
    pub fn record<
        F: FnOnce(
            RecordingCommand<Persistent, Primary, Graphics>,
        ) -> RecordingCommand<Persistent, Primary, Graphics>,
    >(
        &mut self,
        context: &Context,
        f: F,
    ) -> Result<(), ResourceError> {
        let command = self.primary_command.take().unwrap();
        let command = f(context.start_recording(command));
        self.primary_command = Some(command.stop_recording());
        Ok(())
    }
}

pub struct FrameCell<C: RenderPassConfig> {
    cell: Option<Frame<C>>,
}

impl<C: RenderPassConfig> FrameCell<C> {
    #[inline]
    pub fn empty() -> Self {
        FrameCell { cell: None }
    }

    #[inline]
    fn new(frame: Frame<C>) -> Self {
        FrameCell { cell: Some(frame) }
    }

    #[inline]
    pub fn unwrap(mut self) -> Frame<C> {
        self.cell.take().expect("FrameCell is empty")
    }

    #[inline]
    pub fn take(&mut self) -> Self {
        let cell = self.cell.take();
        FrameCell { cell }
    }
}

impl<C: RenderPassConfig> Deref for FrameCell<C> {
    type Target = Frame<C>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.cell.as_ref().expect("FrameCell is empty")
    }
}

impl<C: RenderPassConfig> DerefMut for FrameCell<C> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cell.as_mut().expect("FrameCell is empty")
    }
}

impl<C: RenderPassConfig> FramePool<C> {
    pub fn acquire(
        &mut self,
        context: &Context,
        camera_matrices: &CameraMatrices,
    ) -> Result<FrameCell<C>, ResourceError> {
        let (primary_command, swapchain_frame, camera_descriptor) = context.operate_mut(
            index_list![
                self.swapchain,
                self.camera_uniform.uniform_buffer,
                self.camera_uniform.descriptor_pool,
                self.command_pool
            ],
            |unpack_list![command_pool, descriptor_pool, camera_uniform, swapchain]| {
                let (index, command) = command_pool.next_command();
                // Here begin_primary_command is required to be caled before swapchain get_frame,
                // as begin_command waits for the fence associated with the command execution
                // if the order is reversed, the acquire_next_image will get the semaphore which may have operation still pending
                // this violates the Vulkan spec
                // TODO: Try come up with a pattern that enforces correct order of operations
                let command = context.begin_primary_command(command)?;
                let frame = context.get_frame(swapchain, self.semaphores[index])?;
                let descriptor = descriptor_pool.get(index);
                camera_uniform[index] = *camera_matrices;
                Result::<_, ResourceError>::Ok((command, frame, descriptor))
            },
        )??;
        let frame = Frame {
            swapchain_frame,
            camera_descriptor,
            camera_matrices: *camera_matrices,
            primary_command: Some(primary_command),
        };
        Ok(FrameCell::new(frame))
    }

    pub fn present(&mut self, context: &Context, frame: Frame<C>) -> ResourceResult<()> {
        let Frame {
            primary_command,
            swapchain_frame,
            ..
        } = frame;
        let command = context.finish_command(primary_command.unwrap())?;
        context.operate_mut(index_list![self.swapchain], |unpack_list![swapchain]| {
            context.present_frame(swapchain, command, swapchain_frame)
        })??;
        Ok(())
    }
}

impl<C: RenderPassConfig> Destroy for FrameCell<C> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> DestroyResult<Self> {
        // Frame resources are managed by the FramePool
        let _ = self.cell.take();
        Ok(())
    }
}
