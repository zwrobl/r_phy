use std::{
    convert::Infallible,
    marker::PhantomData,
    ops::{Index, IndexMut},
};

use ash::vk;
use bytemuck::AnyBitPattern;
use type_kit::{Create, CreateResult, Destroy, DestroyResult};

use crate::context::{
    device::{
        command::operation::Operation,
        memory::{AllocReq, HostCoherent},
        raw::allocator::AllocatorIndex,
        resources::{
            buffer::{
                Buffer, BufferBuilder, BufferInfo, PersistentBuffer, PersistentBufferPartial,
            },
            PartialBuilder,
        },
    },
    error::{VkError, VkResult},
    Context,
};

pub struct UniformBuffer<U: AnyBitPattern, O: Operation> {
    len: usize,
    buffer: PersistentBuffer,
    _phantom: PhantomData<(U, O)>,
}

pub struct UniformBufferPartial<U: AnyBitPattern, O: Operation> {
    len: usize,
    buffer: PersistentBufferPartial,
    _phantom: PhantomData<(U, O)>,
}

pub struct UniformBufferBuilder<U: AnyBitPattern, O: Operation> {
    len: usize,
    _phantom: PhantomData<(U, O)>,
}

impl<U: AnyBitPattern, O: Operation> UniformBufferBuilder<U, O> {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            _phantom: PhantomData,
        }
    }
}

impl<'a, U: AnyBitPattern, O: Operation> PartialBuilder<'a> for UniformBufferPartial<U, O> {
    type Config = UniformBufferBuilder<U, O>;
    type Target = UniformBuffer<U, O>;

    fn prepare(config: Self::Config, context: &Context) -> VkResult<Self> {
        let info = BufferInfo {
            size: size_of::<U>() * config.len,
            usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            queue_families: &[O::get_queue_family_index(context)],
        };
        let buffer = PersistentBufferPartial::prepare(BufferBuilder::new(info), context)?;
        Ok(UniformBufferPartial {
            len: config.len,
            buffer,
            _phantom: PhantomData,
        })
    }

    fn requirements(&self) -> impl Iterator<Item = AllocReq> {
        self.buffer.requirements()
    }
}

impl<'a, U: AnyBitPattern, O: Operation> From<&'a UniformBuffer<U, O>>
    for &'a Buffer<HostCoherent>
{
    fn from(value: &'a UniformBuffer<U, O>) -> Self {
        &value.buffer.buffer
    }
}

impl<'a, U: AnyBitPattern, O: Operation> From<&'a mut UniformBuffer<U, O>>
    for &'a mut Buffer<HostCoherent>
{
    fn from(value: &'a mut UniformBuffer<U, O>) -> Self {
        &mut value.buffer.buffer
    }
}

impl<U: AnyBitPattern, O: Operation> Index<usize> for UniformBuffer<U, O> {
    type Output = U;

    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < self.len, "Out of range UniformBuffer access!");
        let ptr = self.buffer.ptr.unwrap() as *mut U;
        unsafe { ptr.add(index).as_ref().unwrap() }
    }
}

impl<U: AnyBitPattern, O: Operation> IndexMut<usize> for UniformBuffer<U, O> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < self.len, "Out of range UniformBuffer access!");
        let ptr = self.buffer.ptr.unwrap() as *mut U;
        unsafe { ptr.add(index).as_mut().unwrap() }
    }
}

impl<U: AnyBitPattern, O: Operation> UniformBuffer<U, O> {
    pub fn handle(&self) -> vk::Buffer {
        self.buffer.buffer.handle()
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

impl<U: AnyBitPattern, O: Operation> Create for UniformBuffer<U, O> {
    type Config<'a> = (UniformBufferPartial<U, O>, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (partial, allocator) = config;
        let len = partial.len;
        let buffer = PersistentBuffer::create((partial.buffer, allocator), context)?;
        Ok(UniformBuffer {
            len,
            buffer,
            _phantom: PhantomData,
        })
    }
}

impl<U: AnyBitPattern, O: Operation> Destroy for UniformBuffer<U, O> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.buffer.destroy(context)?;
        Ok(())
    }
}
