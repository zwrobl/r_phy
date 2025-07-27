use std::{
    convert::Infallible,
    ops::{Deref, DerefMut},
};

use type_kit::{Create, Destroy, DestroyResult, FromGuard};

use crate::{
    device::{
        memory::HostCoherent,
        raw::{
            allocator::AllocatorIndex,
            resources::buffer::{Buffer, BufferPartial, BufferRaw},
        },
    },
    error::ResourceError,
    Context,
};

#[derive(Debug)]
pub struct PersistentBuffer {
    buffer: Buffer<HostCoherent>,
}

impl<'a> From<&'a PersistentBuffer> for &'a Buffer<HostCoherent> {
    #[inline]
    fn from(value: &'a PersistentBuffer) -> Self {
        &value.buffer
    }
}

impl<'a> From<&'a mut PersistentBuffer> for &'a mut Buffer<HostCoherent> {
    #[inline]
    fn from(value: &'a mut PersistentBuffer) -> Self {
        &mut value.buffer
    }
}

impl Deref for PersistentBuffer {
    type Target = Buffer<HostCoherent>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for PersistentBuffer {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl FromGuard for PersistentBuffer {
    type Inner = BufferRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self.buffer.into_inner()
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            buffer: Buffer::<HostCoherent>::from_inner(inner),
        }
    }
}

impl Create for PersistentBuffer {
    type Config<'a> = (BufferPartial<HostCoherent>, AllocatorIndex);
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let mut buffer = Buffer::create(config, context)?;
        buffer.map_memory(context)?;
        Ok(PersistentBuffer { buffer })
    }
}

impl Destroy for PersistentBuffer {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = context.unmap_allocation(self.buffer.allocation);
        self.buffer.destroy(context)
    }
}
