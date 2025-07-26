mod writer;

use std::{
    any::{type_name, TypeId},
    convert::Infallible,
    error::Error,
    marker::PhantomData,
};

use type_kit::{Create, Destroy, DestroyResult, FromGuard};
pub use writer::*;

use ash::vk;

use crate::context::{
    device::{
        raw::resources::pipeline::{GraphicsPipeline, GraphicsPipelineConfig},
        raw::unique::layout::{DescriptorLayout, DescriptorSetLayout, Layout},
    },
    error::ResourceError,
    Context,
};

#[derive(Debug, Clone)]
pub struct DescriptorPoolData {
    pool: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
}

impl Destroy for DescriptorPoolData {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_descriptor_pool(self.pool, None);
        };
        Ok(())
    }
}

#[derive(Debug)]
pub struct DescriptorPool<T: DescriptorLayout> {
    data: DescriptorPoolData,
    _phantom: PhantomData<T>,
}

impl<'a, T: DescriptorLayout> From<&'a DescriptorPool<T>> for &'a DescriptorPoolData {
    fn from(pool: &'a DescriptorPool<T>) -> Self {
        &pool.data
    }
}

impl<'a, T: DescriptorLayout> From<&'a mut DescriptorPool<T>> for &'a mut DescriptorPoolData {
    fn from(pool: &'a mut DescriptorPool<T>) -> Self {
        &mut pool.data
    }
}

#[derive(Debug)]
pub struct Descriptor<T: DescriptorLayout> {
    set: vk::DescriptorSet,
    _phantom: PhantomData<T>,
}

impl<T: DescriptorLayout> Clone for Descriptor<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: DescriptorLayout> Copy for Descriptor<T> {}

impl<T: DescriptorLayout> From<Descriptor<T>> for vk::DescriptorSet {
    fn from(descriptor: Descriptor<T>) -> Self {
        descriptor.set
    }
}

impl<T: DescriptorLayout> DescriptorPool<T> {
    pub fn get(&self, index: usize) -> Descriptor<T> {
        Descriptor {
            set: self.data.sets[index],
            _phantom: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.data.sets.len()
    }
}

#[derive(Debug)]
pub struct DescriptorPoolRef<'a, T: DescriptorLayout> {
    data: &'a DescriptorPoolData,
    _phantom: PhantomData<T>,
}

impl<'a, T: DescriptorLayout, N: DescriptorLayout> TryFrom<&'a DescriptorPool<T>>
    for DescriptorPoolRef<'a, N>
{
    type Error = &'static str;

    fn try_from(pool: &'a DescriptorPool<T>) -> Result<DescriptorPoolRef<'a, N>, Self::Error> {
        if TypeId::of::<T>() == TypeId::of::<N>() {
            Ok(DescriptorPoolRef {
                data: &pool.data,
                _phantom: PhantomData,
            })
        } else {
            Err("Invalid DescriptorLayout type")
        }
    }
}

impl<'a, T: DescriptorLayout> DescriptorPoolRef<'a, T> {
    pub fn get(&self, index: usize) -> Descriptor<T> {
        Descriptor {
            set: self.data.sets[index],
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct DescriptorBindingData {
    pub set_index: u32,
    pub set: vk::DescriptorSet,
    pub pipeline_layout: vk::PipelineLayout,
}

impl<T: DescriptorLayout> Descriptor<T> {
    pub fn get_binding_data<C: GraphicsPipelineConfig>(
        &self,
        pipeline: &GraphicsPipeline<C>,
    ) -> Result<DescriptorBindingData, Box<dyn Error>> {
        let set_index = C::Layout::sets().get_set_index::<T>().unwrap_or_else(|| {
            panic!(
                "DescriptorSet {} not present in layout DescriptorSets {}",
                type_name::<T>(),
                type_name::<<C::Layout as Layout>::Descriptors>()
            )
        });
        Ok(DescriptorBindingData {
            set_index,
            set: self.set,
            pipeline_layout: pipeline.layout().into(),
        })
    }
}

// impl<L: DescriptorLayout> Resource for DescriptorPool<L> {
//     type RawType = DescriptorPoolData;
// }

impl<L: DescriptorLayout> FromGuard for DescriptorPool<L> {
    type Inner = DescriptorPoolData;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        self.data
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            data: inner,
            _phantom: PhantomData,
        }
    }
}

impl<L: DescriptorLayout> Create for DescriptorPool<L> {
    type Config<'a> = DescriptorSetWriter<L>;
    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let pool_sizes = L::get_descriptor_pool_sizes(config.num_sets() as u32);
        let pool_create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(config.num_sets() as u32);
        let pool = unsafe {
            context
                .device
                .create_descriptor_pool(&pool_create_info, None)?
        };
        let layout = context.get_or_create_unique_resource::<DescriptorSetLayout<L>, _>()?;
        let sets = unsafe {
            context.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::builder()
                    .descriptor_pool(pool)
                    .set_layouts(&vec![layout.layout; config.num_sets()]),
            )?
        };
        let sets = context
            .write_descriptors(config, sets)
            .into_iter()
            .map(Into::<vk::DescriptorSet>::into)
            .collect();
        Ok(DescriptorPool {
            data: DescriptorPoolData { pool, sets },
            _phantom: PhantomData,
        })
    }
}

impl<L: DescriptorLayout> Destroy for DescriptorPool<L> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_descriptor_pool(self.data.pool, None);
        };
        Ok(())
    }
}
