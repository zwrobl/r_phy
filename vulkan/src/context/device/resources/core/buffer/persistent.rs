use std::{convert::Infallible, ffi::c_void};

use type_kit::{Create, Destroy, DestroyResult};

use crate::context::{
    device::{
        memory::{AllocReq, HostCoherent},
        raw::allocator::AllocatorIndex,
        resources::PartialBuilder,
    },
    error::{VkError, VkResult},
    Context,
};

use super::{Buffer, BufferBuilder, BufferPartial};

pub struct PersistentBufferPartial {
    buffer: BufferPartial<HostCoherent>,
}

pub struct PersistentBuffer {
    pub buffer: Buffer<HostCoherent>,
    pub ptr: Option<*mut c_void>,
}

impl<'a> From<&'a PersistentBuffer> for &'a Buffer<HostCoherent> {
    fn from(value: &'a PersistentBuffer) -> Self {
        &value.buffer
    }
}

impl<'a> From<&'a mut PersistentBuffer> for &'a mut Buffer<HostCoherent> {
    fn from(value: &'a mut PersistentBuffer) -> Self {
        &mut value.buffer
    }
}

impl<'a> PartialBuilder<'a> for PersistentBufferPartial {
    type Config = BufferBuilder<'a, HostCoherent>;
    type Target = PersistentBuffer;

    fn prepare(config: Self::Config, context: &Context) -> VkResult<Self> {
        let buffer = BufferPartial::prepare(config, context)?;
        Ok(PersistentBufferPartial { buffer })
    }

    fn requirements(&self) -> impl Iterator<Item = AllocReq> {
        self.buffer.requirements()
    }
}

impl Create for PersistentBuffer {
    type Config<'a> = (PersistentBufferPartial, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (partial, allocator) = config;
        let buffer = Buffer::create((partial.buffer, allocator), context)?;
        let ptr = context.map_allocation(buffer.allocation)?;
        Ok(PersistentBuffer {
            buffer,
            ptr: Some(ptr),
        })
    }
}

impl Destroy for PersistentBuffer {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = context.unmap_allocation(self.buffer.allocation);
        let _ = self.buffer.destroy(context)?;
        Ok(())
    }
}
