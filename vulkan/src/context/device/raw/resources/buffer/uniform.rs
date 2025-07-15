use std::{
    convert::Infallible,
    marker::PhantomData,
    ops::{Deref, DerefMut, Index, IndexMut},
};

use ash::vk;
use bytemuck::AnyBitPattern;
use type_kit::{Create, Destroy, DestroyResult, FromGuard};

use crate::context::{
    device::{
        command::operation::Operation,
        memory::HostCoherent,
        raw::{
            allocator::AllocatorIndex,
            resources::buffer::{
                persistent::PersistentBuffer, Buffer, BufferInfoBuilder, BufferPartial, BufferRaw,
            },
            Partial,
        },
    },
    error::ResourceError,
    Context,
};

#[derive(Debug)]
pub struct UniformBufferInfoBuilder<U: AnyBitPattern, O: Operation> {
    len: Option<usize>,
    _phantom: PhantomData<(U, O)>,
}

#[derive(Debug)]
pub struct UniformBufferInfo<U: AnyBitPattern, O: Operation> {
    len: usize,
    queue_indices: [u32; 1],
    _phantom: PhantomData<(U, O)>,
}

impl<U: AnyBitPattern, O: Operation> UniformBufferInfoBuilder<U, O> {
    #[inline]
    pub fn new() -> Self {
        Self {
            len: None,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn with_len(self, len: usize) -> Self {
        Self {
            len: Some(len),
            ..self
        }
    }

    #[inline]
    fn build(self, context: &Context) -> UniformBufferInfo<U, O> {
        UniformBufferInfo {
            len: self.len.unwrap(),
            queue_indices: [O::get_queue_family_index(context)],
            _phantom: PhantomData,
        }
    }
}

impl<U: AnyBitPattern, O: Operation> UniformBufferInfo<U, O> {
    #[inline]
    fn get_buffer_info<'a>(&'a self) -> BufferInfoBuilder<'a, HostCoherent> {
        BufferInfoBuilder::<HostCoherent>::new()
            .with_queue_families(&self.queue_indices)
            .with_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .with_size((self.len * size_of::<U>()) as vk::DeviceSize)
            .with_usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
    }
}

#[derive(Debug)]
pub struct UniformBufferPartial<U: AnyBitPattern, O: Operation> {
    partial: BufferPartial<HostCoherent>,
    _phantom: PhantomData<(U, O)>,
}

impl<U: AnyBitPattern, O: Operation> Partial for UniformBufferPartial<U, O> {
    fn register_memory_requirements<B: crate::context::device::raw::allocator::AllocatorBuilder>(
        &self,
        builder: &mut B,
    ) {
        self.partial.register_memory_requirements(builder);
    }
}

impl<U: AnyBitPattern, O: Operation> Create for UniformBufferPartial<U, O> {
    type Config<'a> = UniformBufferInfoBuilder<U, O>;

    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let partial = BufferPartial::create(config.build(context).get_buffer_info(), context)?;
        Ok(UniformBufferPartial {
            partial,
            _phantom: PhantomData,
        })
    }
}

impl<U: AnyBitPattern, O: Operation> Destroy for UniformBufferPartial<U, O> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.partial.destroy(context)
    }
}

#[derive(Debug)]
pub struct UniformBuffer<U: AnyBitPattern, O: Operation> {
    buffer: PersistentBuffer,
    _phantom: PhantomData<(U, O)>,
}

impl<U: AnyBitPattern, O: Operation> UniformBuffer<U, O> {
    #[inline]
    pub fn len(&self) -> usize {
        self.get_size() / size_of::<U>()
    }
}

impl<'a, U: AnyBitPattern, O: Operation> From<&'a UniformBuffer<U, O>>
    for &'a Buffer<HostCoherent>
{
    #[inline]
    fn from(value: &'a UniformBuffer<U, O>) -> Self {
        &value.buffer
    }
}

impl<'a, U: AnyBitPattern, O: Operation> From<&'a mut UniformBuffer<U, O>>
    for &'a mut Buffer<HostCoherent>
{
    #[inline]
    fn from(value: &'a mut UniformBuffer<U, O>) -> Self {
        &mut value.buffer
    }
}

impl<U: AnyBitPattern, O: Operation> Deref for UniformBuffer<U, O> {
    type Target = Buffer<HostCoherent>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl<U: AnyBitPattern, O: Operation> DerefMut for UniformBuffer<U, O> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl<U: AnyBitPattern, O: Operation> Create for UniformBuffer<U, O> {
    type Config<'a> = (UniformBufferPartial<U, O>, AllocatorIndex);
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let (UniformBufferPartial { partial, .. }, allocator) = config;
        let buffer = PersistentBuffer::create((partial, allocator), context)?;
        Ok(UniformBuffer {
            buffer,
            _phantom: PhantomData,
        })
    }
}

impl<U: AnyBitPattern, O: Operation> Destroy for UniformBuffer<U, O> {
    type Context<'a> = &'a Context;

    type DestroyError = ResourceError;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.buffer.destroy(context);
        Ok(())
    }
}

impl<U: AnyBitPattern, O: Operation> FromGuard for UniformBuffer<U, O> {
    type Inner = BufferRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self.buffer.into_inner()
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            buffer: PersistentBuffer::from_inner(inner),
            _phantom: PhantomData,
        }
    }
}

impl<U: AnyBitPattern, O: Operation> Index<usize> for UniformBuffer<U, O> {
    type Output = U;

    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < self.len(), "Out of range UniformBuffer access!");
        let ptr = self.buffer.ptr.unwrap() as *mut U;
        unsafe { ptr.add(index).as_ref().unwrap() }
    }
}

impl<U: AnyBitPattern, O: Operation> IndexMut<usize> for UniformBuffer<U, O> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < self.len(), "Out of range UniformBuffer access!");
        let ptr = self.buffer.ptr.unwrap() as *mut U;
        unsafe { ptr.add(index).as_mut().unwrap() }
    }
}
