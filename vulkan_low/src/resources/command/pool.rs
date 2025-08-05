mod level;

pub use level::*;

use std::{convert::Infallible, marker::PhantomData};

use ash::vk;
use type_kit::{Create, CreateResult, Destroy, DestroyResult, DropGuard, FromGuard, GuardVec};

use crate::{
    error::ExtResult,
    resources::{
        command::{Command, NewCommand, Operation, Persistent, Transient},
        error::{GuardError, ResourceError},
        storage::TypeUniqueResource,
        Resource, ResourceGuardError,
    },
    Context,
};

#[derive(Debug, Clone, Copy)]
pub struct PersistentCommandPoolRaw {
    command_pool: vk::CommandPool,
    allocator: PersistentAllocatorRaw,
}

impl<L: Level, O: Operation> Resource for PersistentCommandPool<L, O> {
    type RawType = PersistentCommandPoolRaw;
    type RawCollection = GuardVec<Self::RawType>;

    #[inline]
    fn wrap_guard_error((resource, err): ResourceGuardError<Self>) -> ResourceError {
        ResourceError::GuardError(GuardError::PersistentCommandPool {
            error: (DropGuard::new(resource), err),
        })
    }
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
    pub fn next_command(&mut self) -> (usize, NewCommand<Persistent, L, O>) {
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
                    .queue_family_index(O::get_queue_family_index(context))
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
    ) -> ExtResult<NewCommand<Transient, Primary, O>> {
        let pool = O::get_transient_command_pool(self);
        let &buffer = unsafe {
            self.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::builder()
                    .level(Primary::LEVEL)
                    .command_pool(pool.pool)
                    .command_buffer_count(1),
            )?
            .first()
            .unwrap()
        };
        let fence = unsafe {
            self.create_fence(
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
            self.free_command_buffers(pool.pool, &[buffer]);
            self.destroy_fence(fence, None);
        }
    }
}
