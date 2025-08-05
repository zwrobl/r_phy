mod writer;

use std::{
    any::TypeId,
    convert::Infallible,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use type_kit::{Contains, Create, Destroy, DestroyResult, DropGuard, FromGuard, GuardVec, Marker};
pub use writer::*;

use ash::vk;

use crate::{
    resources::{
        error::{GuardError, ResourceError},
        layout::{
            DescriptorIndex, DescriptorLayout, DescriptorLayoutList, DescriptorSetLayout, Layout,
        },
        pipeline::{GraphicsPipeline, GraphicsPipelineConfig},
        Resource, ResourceGuardError,
    },
    Context,
};

#[derive(Debug, Clone, Copy)]
pub struct DescriptorPoolDataRaw {
    pool: vk::DescriptorPool,
    sets: Option<NonNull<[vk::DescriptorSet]>>,
}

#[derive(Debug)]
pub struct DescriptorPoolData {
    pool: vk::DescriptorPool,
    sets: Box<[vk::DescriptorSet]>,
}

#[derive(Debug)]
pub struct DescriptorPool<T: DescriptorLayout> {
    data: DescriptorPoolData,
    _phantom: PhantomData<T>,
}

impl<T: DescriptorLayout> Deref for DescriptorPool<T> {
    type Target = DescriptorPoolData;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: DescriptorLayout> DerefMut for DescriptorPool<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
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
            set: self.sets[index],
            _phantom: PhantomData,
        }
    }

    pub fn size(&self) -> usize {
        self.sets.len()
    }
}

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

pub type PipelineSetIndices<C> = <<<C as GraphicsPipelineConfig>::Layout as Layout>::Descriptors as DescriptorLayoutList>::IndexList;

impl<T: DescriptorLayout> Descriptor<T> {
    pub fn get_binding<C: GraphicsPipelineConfig, M: Marker>(
        &self,
        pipeline: &GraphicsPipeline<C>,
    ) -> DescriptorBindingData
    where
        PipelineSetIndices<C>: Contains<DescriptorIndex<T>, M>,
    {
        let set_index = C::Layout::sets().get_index::<T, _>();
        DescriptorBindingData {
            set_index,
            set: self.set,
            pipeline_layout: pipeline.layout().into(),
        }
    }

    pub fn try_get_binding<C: GraphicsPipelineConfig>(
        &self,
        pipeline: &GraphicsPipeline<C>,
    ) -> Option<DescriptorBindingData> {
        C::Layout::sets()
            .try_get_index::<T>()
            .and_then(|set_index| {
                Some(DescriptorBindingData {
                    set_index,
                    set: self.set,
                    pipeline_layout: pipeline.layout().into(),
                })
            })
    }
}

impl<L: DescriptorLayout> Resource for DescriptorPool<L> {
    type RawType = DescriptorPoolDataRaw;
    type RawCollection = GuardVec<Self::RawType>;

    #[inline]
    fn wrap_guard_error((resource, err): ResourceGuardError<Self>) -> ResourceError {
        ResourceError::GuardError(GuardError::DescriptorPool {
            error: (DropGuard::new(resource), err),
        })
    }
}

impl<L: DescriptorLayout> FromGuard for DescriptorPool<L> {
    type Inner = DescriptorPoolDataRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner {
            pool: self.data.pool,
            sets: NonNull::new(Box::leak(self.data.sets)),
        }
    }

    #[inline]
    unsafe fn from_inner(mut inner: Self::Inner) -> Self {
        Self {
            data: DescriptorPoolData {
                pool: inner.pool,
                sets: unsafe { Box::from_raw(inner.sets.take().unwrap().as_mut()) },
            },
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
        let pool_sizes = L::get_descriptor_pools(config.num_sets() as u32);
        let pool_create_info = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(config.num_sets() as u32);
        let pool = unsafe {
            context
                .device
                .create_descriptor_pool(&pool_create_info, None)?
        };
        let layout = context.get_unique_resource::<DescriptorSetLayout<L>, _>()?;
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
            context.destroy_descriptor_pool(self.pool, None);
        };
        Ok(())
    }
}

impl Destroy for DescriptorPoolDataRaw {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_descriptor_pool(self.pool, None);
        };
        if let Some(mut sets) = self.sets.take() {
            drop(unsafe { Box::from_raw(sets.as_mut()) });
        }
        Ok(())
    }
}
