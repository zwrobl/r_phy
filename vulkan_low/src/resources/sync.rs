use std::{convert::Infallible, ops::Deref};

use ash::vk;
use type_kit::{Create, Destroy, DestroyResult};

use crate::Context;

#[derive(Debug, Clone, Copy)]
pub struct Semaphore {
    handle: vk::Semaphore,
}

impl Deref for Semaphore {
    type Target = vk::Semaphore;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl Create for Semaphore {
    type Config<'a> = ();
    type CreateError = vk::Result;

    #[inline]
    fn create<'a, 'b>(
        _: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let handle =
            unsafe { context.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)? };
        Ok(Self { handle })
    }
}

impl Destroy for Semaphore {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, context: Self::Context<'_>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_semaphore(self.handle, None);
        }
        Ok(())
    }
}
