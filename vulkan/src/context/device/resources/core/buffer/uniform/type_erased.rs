use std::{
    any::{type_name, TypeId},
    convert::Infallible,
    error::Error,
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

pub struct UniformBufferErasedPartial<O: Operation> {
    len: usize,
    buffer: PersistentBufferPartial,
    item_type_id: TypeId,
    _phantom: PhantomData<O>,
}

pub struct UniformBufferErasedBuilder<O: Operation> {
    len: usize,
    item_size: usize,
    item_type_id: TypeId,
    _phantom: PhantomData<O>,
}

impl<O: Operation> UniformBufferErasedBuilder<O> {
    pub fn new<U: AnyBitPattern>(len: usize) -> Self {
        Self {
            len,
            item_size: size_of::<U>(),
            item_type_id: TypeId::of::<U>(),
            _phantom: PhantomData,
        }
    }
}

impl<'a, O: Operation> PartialBuilder<'a> for UniformBufferErasedPartial<O> {
    type Config = UniformBufferErasedBuilder<O>;
    type Target = UniformBufferTypeErased<O>;

    fn prepare(config: Self::Config, context: &Context) -> VkResult<Self> {
        let UniformBufferErasedBuilder {
            len,
            item_size,
            item_type_id,
            ..
        } = config;
        let info = BufferInfo {
            size: item_size * config.len,
            usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            queue_families: &[O::get_queue_family_index(context)],
        };
        let buffer = PersistentBufferPartial::prepare(BufferBuilder::new(info), context)?;
        Ok(UniformBufferErasedPartial {
            len,
            buffer,
            item_type_id,
            _phantom: PhantomData,
        })
    }

    fn requirements(&self) -> impl Iterator<Item = AllocReq> {
        self.buffer.requirements()
    }
}

pub struct UniformBufferTypeErased<O: Operation> {
    len: usize,
    buffer: PersistentBuffer,
    item_type_id: TypeId,
    _phantom: PhantomData<O>,
}

pub struct UniformBufferRef<'a, P: AnyBitPattern, O: Operation> {
    len: usize,
    buffer: &'a mut PersistentBuffer,
    _phantom: PhantomData<(P, O)>,
}

impl<'a, P: AnyBitPattern, O: Operation> TryFrom<&'a mut UniformBufferTypeErased<O>>
    for UniformBufferRef<'a, P, O>
{
    type Error = Box<dyn Error>;

    fn try_from(value: &'a mut UniformBufferTypeErased<O>) -> Result<Self, Self::Error> {
        if value.item_type_id == TypeId::of::<P>() {
            Ok(UniformBufferRef {
                len: value.len,
                buffer: &mut value.buffer,
                _phantom: PhantomData,
            })
        } else {
            Err(format!(
                "Invalid uniform data type {} for uniform buffer!",
                type_name::<P>()
            ))?
        }
    }
}

impl<'a, O: Operation> From<&'a mut UniformBufferTypeErased<O>> for &'a mut Buffer<HostCoherent> {
    fn from(value: &'a mut UniformBufferTypeErased<O>) -> Self {
        (&mut value.buffer).into()
    }
}

impl<U: AnyBitPattern, O: Operation> Index<usize> for UniformBufferRef<'_, U, O> {
    type Output = U;

    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < self.len, "Out of range UniformBuffer access!");
        let ptr = self.buffer.ptr.unwrap() as *mut U;
        unsafe { ptr.add(index).as_ref().unwrap() }
    }
}

impl<U: AnyBitPattern, O: Operation> IndexMut<usize> for UniformBufferRef<'_, U, O> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < self.len, "Out of range UniformBuffer access!");
        let ptr = self.buffer.ptr.unwrap() as *mut U;
        unsafe { ptr.add(index).as_mut().unwrap() }
    }
}

impl<O: Operation> Create for UniformBufferTypeErased<O> {
    type Config<'a> = (UniformBufferErasedPartial<O>, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (
            UniformBufferErasedPartial {
                len,
                buffer,
                item_type_id,
                ..
            },
            allocator,
        ) = config;
        let buffer = PersistentBuffer::create((buffer, allocator), context)?;
        Ok(UniformBufferTypeErased {
            len,
            buffer,
            item_type_id,
            _phantom: PhantomData,
        })
    }
}

impl<O: Operation> Destroy for UniformBufferTypeErased<O> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.buffer.destroy(context)?;
        Ok(())
    }
}
