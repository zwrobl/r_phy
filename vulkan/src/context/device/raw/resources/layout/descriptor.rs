use std::{any::TypeId, collections::HashMap, convert::Infallible, marker::PhantomData};

use ash::vk;

use crate::context::{device::raw::resources::TypeUniqueResource, error::ResourceError, Context};
use type_kit::{Cons, Create, Destroy, FromGuard, Nil};

pub trait DescriptorBinding: 'static {
    fn has_data() -> bool;

    fn get_descriptor_set_binding(binding: u32) -> vk::DescriptorSetLayoutBinding;

    fn get_descriptor_write(binding: u32) -> vk::WriteDescriptorSet;

    fn get_descriptor_pool_size(num_sets: u32) -> vk::DescriptorPoolSize;
}

pub trait DescriptorLayout: 'static {
    fn get_descriptor_set_bindings() -> Vec<vk::DescriptorSetLayoutBinding>;

    fn get_descriptor_writes<T: DescriptorBinding>() -> Vec<vk::WriteDescriptorSet>;

    fn get_descriptor_pool_sizes(num_sets: u32) -> Vec<vk::DescriptorPoolSize>;
}

pub trait DescriptorBindingList: 'static {
    const LEN: usize;

    type Item: DescriptorBinding;
    type Next: DescriptorBindingList;
}

impl DescriptorBinding for Nil {
    fn has_data() -> bool {
        unreachable!()
    }

    fn get_descriptor_set_binding(_binding: u32) -> vk::DescriptorSetLayoutBinding {
        unreachable!()
    }

    fn get_descriptor_write(_binding: u32) -> vk::WriteDescriptorSet {
        unreachable!()
    }

    fn get_descriptor_pool_size(_num_sets: u32) -> vk::DescriptorPoolSize {
        unreachable!()
    }
}

impl DescriptorBindingList for Nil {
    const LEN: usize = 0;

    type Item = Self;
    type Next = Self;
}

impl<B: DescriptorBinding, N: DescriptorBindingList> DescriptorBindingList for Cons<B, N> {
    const LEN: usize = N::LEN + 1;

    type Item = B;
    type Next = N;
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorLayoutBuilder<B: DescriptorBindingList> {
    _phantom: PhantomData<B>,
}

impl<B: DescriptorBindingList> Default for DescriptorLayoutBuilder<B> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

#[allow(dead_code)]
impl<B: DescriptorBindingList> DescriptorLayoutBuilder<B> {
    pub fn new() -> DescriptorLayoutBuilder<Nil> {
        DescriptorLayoutBuilder {
            _phantom: PhantomData,
        }
    }

    pub fn push<N: DescriptorBinding>(self) -> DescriptorLayoutBuilder<Cons<N, B>> {
        DescriptorLayoutBuilder {
            _phantom: PhantomData,
        }
    }

    pub fn builder() -> Self {
        Self::default()
    }

    fn next_descriptor_binding<'a, T: DescriptorBindingList>(
        binding: u32,
        mut descriptor_bindings: Vec<vk::DescriptorSetLayoutBinding>,
    ) -> Vec<vk::DescriptorSetLayoutBinding> {
        if T::LEN > 0 {
            let next_binding = if T::Item::has_data() {
                descriptor_bindings.push(T::Item::get_descriptor_set_binding(binding));
                binding + 1
            } else {
                binding
            };
            Self::next_descriptor_binding::<T::Next>(next_binding, descriptor_bindings)
        } else {
            descriptor_bindings
        }
    }

    pub fn get_descriptor_bindings() -> Vec<vk::DescriptorSetLayoutBinding> {
        Self::next_descriptor_binding::<B>(0, Vec::with_capacity(B::LEN))
    }

    fn try_get_descriptor_writes<S: DescriptorBinding, T: DescriptorBindingList>(
        binding: u32,
        mut vec: Vec<vk::WriteDescriptorSet>,
    ) -> Vec<vk::WriteDescriptorSet> {
        debug_assert!(S::has_data(), "DescriptorBinding has no data!");
        if T::LEN > 0 {
            if T::Item::has_data() {
                if TypeId::of::<S>() == TypeId::of::<T::Item>() {
                    vec.push(T::Item::get_descriptor_write(binding));
                }
                Self::try_get_descriptor_writes::<S, T::Next>(binding + 1, vec)
            } else {
                Self::try_get_descriptor_writes::<S, T::Next>(binding, vec)
            }
        } else {
            vec
        }
    }

    pub fn get_descriptor_writes<T: DescriptorBinding>() -> Vec<vk::WriteDescriptorSet> {
        Self::try_get_descriptor_writes::<T, B>(0, Vec::new())
    }

    fn next_descriptor_pool_size<T: DescriptorBindingList>(
        num_sets: u32,
        pool_sizes: &mut HashMap<vk::DescriptorType, u32>,
    ) {
        if T::LEN > 0 {
            if T::Item::has_data() {
                let pool_size = T::Item::get_descriptor_pool_size(num_sets);
                let descriptor_count = pool_sizes.entry(pool_size.ty).or_insert(0);
                *descriptor_count += pool_size.descriptor_count;
            }
            Self::next_descriptor_pool_size::<T::Next>(num_sets, pool_sizes);
        }
    }

    pub fn get_descriptor_pool_sizes(num_sets: u32) -> Vec<vk::DescriptorPoolSize> {
        let mut pool_sizes = HashMap::new();
        Self::next_descriptor_pool_size::<B>(num_sets, &mut pool_sizes);
        pool_sizes
            .into_iter()
            .map(|(ty, descriptor_count)| vk::DescriptorPoolSize {
                ty,
                descriptor_count,
            })
            .collect()
    }
}

impl<B: DescriptorBindingList> DescriptorLayout for DescriptorLayoutBuilder<B> {
    fn get_descriptor_set_bindings() -> Vec<vk::DescriptorSetLayoutBinding> {
        Self::get_descriptor_bindings()
    }

    fn get_descriptor_writes<T: DescriptorBinding>() -> Vec<vk::WriteDescriptorSet> {
        Self::get_descriptor_writes::<T>()
    }

    fn get_descriptor_pool_sizes(num_sets: u32) -> Vec<vk::DescriptorPoolSize> {
        Self::get_descriptor_pool_sizes(num_sets)
    }
}

#[derive(Debug)]
pub struct DescriptorSetLayout<T: DescriptorLayout> {
    pub layout: vk::DescriptorSetLayout,
    _phantom: PhantomData<T>,
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorSetLayoutRaw {
    layout: vk::DescriptorSetLayout,
}

impl<T: DescriptorLayout> TypeUniqueResource for DescriptorSetLayout<T> {
    type RawType = DescriptorSetLayoutRaw;
}

impl<T: DescriptorLayout> FromGuard for DescriptorSetLayout<T> {
    type Inner = DescriptorSetLayoutRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner {
            layout: self.layout,
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            layout: inner.layout,
            _phantom: PhantomData,
        }
    }
}

impl<T: DescriptorLayout> Create for DescriptorSetLayout<T> {
    type Config<'a> = ();

    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(
        _config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let layout = unsafe {
            context.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::builder()
                    .bindings(&T::get_descriptor_set_bindings()),
                None,
            )?
        };
        Ok(Self {
            layout,
            _phantom: PhantomData,
        })
    }
}

impl<T: DescriptorLayout> Destroy for DescriptorSetLayout<T> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        unsafe {
            context.destroy_descriptor_set_layout(self.layout, None);
        }
        Ok(())
    }
}

impl Destroy for DescriptorSetLayoutRaw {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        unsafe {
            context.destroy_descriptor_set_layout(self.layout, None);
        }
        Ok(())
    }
}
