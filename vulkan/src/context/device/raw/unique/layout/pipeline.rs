use std::{any::TypeId, convert::Infallible, marker::PhantomData};

use ash::vk;

use crate::context::{
    device::raw::unique::{
        layout::{DescriptorBinding, DescriptorLayout, DescriptorSetLayout},
        TypeUniqueResource,
    },
    error::{ResourceError, ResourceResult},
    Context,
};
use type_kit::{Cons, Create, Destroy, FromGuard, Nil};

pub trait Layout: 'static {
    type Descriptors: DescriptorLayoutList;
    type PushConstants: PushConstantList;

    fn ranges() -> PushConstantRanges<Self::PushConstants> {
        PushConstantRanges::<Self::PushConstants>::builder()
    }

    fn sets() -> DescriptorSets<Self::Descriptors> {
        DescriptorSets::<Self::Descriptors>::builder()
    }
}

pub trait PushConstant: 'static {
    fn range(offset: u32) -> vk::PushConstantRange;
}

pub trait PushConstantList: 'static {
    type Item: PushConstant;
    type Next: PushConstantList;

    fn exhausted() -> bool;
    fn len() -> usize;
}

impl PushConstant for Nil {
    fn range(_offset: u32) -> vk::PushConstantRange {
        unreachable!()
    }
}

impl PushConstantList for Nil {
    type Item = Self;
    type Next = Self;

    fn exhausted() -> bool {
        true
    }

    fn len() -> usize {
        0
    }
}

impl<P: PushConstant, N: PushConstantList> PushConstantList for Cons<P, N> {
    type Item = P;
    type Next = N;

    fn exhausted() -> bool {
        false
    }

    fn len() -> usize {
        Self::Next::len() + 1
    }
}

pub struct PushConstantRanges<N: PushConstantList> {
    _phantom: PhantomData<N>,
}

impl<N: PushConstantList> Default for PushConstantRanges<N> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

#[allow(dead_code)]
impl<N: PushConstantList> PushConstantRanges<N> {
    pub fn new() -> PushConstantRanges<Nil> {
        PushConstantRanges {
            _phantom: PhantomData,
        }
    }

    pub fn push<P: PushConstant>(self) -> PushConstantRanges<Cons<P, N>> {
        PushConstantRanges {
            _phantom: PhantomData,
        }
    }

    pub fn builder() -> Self {
        Self::default()
    }

    fn next_push_range<'a, T: PushConstantList>(
        offset: u32,
        mut iter: impl Iterator<Item = &'a mut vk::PushConstantRange>,
    ) {
        if !T::exhausted() {
            if let Some(entry) = iter.next() {
                let range = T::Item::range(offset);
                *entry = range;
                Self::next_push_range::<T::Next>(offset + range.size, iter)
            }
        }
    }

    pub fn get_ranges() -> Vec<vk::PushConstantRange> {
        let mut ranges = vec![vk::PushConstantRange::default(); N::len()];
        Self::next_push_range::<N>(0, ranges.iter_mut());
        ranges
    }

    fn try_get_next_range<P: PushConstant, L: PushConstantList>(
        offset: u32,
    ) -> Option<vk::PushConstantRange> {
        if !L::exhausted() {
            let range = L::Item::range(offset);
            if TypeId::of::<P>() == TypeId::of::<L::Item>() {
                Some(range)
            } else {
                Self::try_get_next_range::<P, L::Next>(offset + range.size)
            }
        } else {
            None
        }
    }

    pub fn try_get_range<P: PushConstant>(&self) -> Option<vk::PushConstantRange> {
        Self::try_get_next_range::<P, N>(0)
    }
}

pub trait DescriptorLayoutList: 'static {
    type Item: DescriptorLayout;
    type Next: DescriptorLayoutList;

    fn exhausted() -> bool;
    fn len() -> usize;
}

impl DescriptorLayoutList for Nil {
    type Item = Self;
    type Next = Self;

    fn exhausted() -> bool {
        true
    }

    fn len() -> usize {
        0
    }
}

impl DescriptorLayout for Nil {
    fn get_descriptor_set_bindings() -> Vec<vk::DescriptorSetLayoutBinding> {
        unreachable!()
    }

    fn get_descriptor_pool_sizes(_num_sets: u32) -> Vec<vk::DescriptorPoolSize> {
        unreachable!()
    }

    fn get_descriptor_writes<T: DescriptorBinding>() -> Vec<vk::WriteDescriptorSet> {
        unreachable!()
    }
}

impl<L: DescriptorLayout, N: DescriptorLayoutList> DescriptorLayoutList for Cons<L, N> {
    type Item = L;
    type Next = N;

    fn exhausted() -> bool {
        false
    }

    fn len() -> usize {
        Self::Next::len() + 1
    }
}

pub struct DescriptorSets<L: DescriptorLayoutList> {
    _phantom: PhantomData<L>,
}

impl<L: DescriptorLayoutList> DescriptorSets<L> {
    pub fn builder() -> DescriptorSets<L> {
        DescriptorSets {
            _phantom: PhantomData,
        }
    }

    fn try_get_index<T: DescriptorLayout, N: DescriptorLayoutList>(index: u32) -> Option<u32> {
        if !N::exhausted() {
            if TypeId::of::<T>() == TypeId::of::<N::Item>() {
                Some(index - 1)
            } else {
                Self::try_get_index::<T, N::Next>(index - 1)
            }
        } else {
            None
        }
    }

    pub fn get_set_index<T: DescriptorLayout>(&self) -> Option<u32> {
        Self::try_get_index::<T, L>(L::len() as u32)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PipelineLayoutBuilder<T: DescriptorLayoutList, P: PushConstantList> {
    _phantom: PhantomData<(T, P)>,
}

impl<T: DescriptorLayoutList, P: PushConstantList> Default for PipelineLayoutBuilder<T, P> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

#[allow(dead_code)]
impl<T: DescriptorLayoutList, P: PushConstantList> PipelineLayoutBuilder<T, P> {
    pub fn new() -> PipelineLayoutBuilder<Nil, Nil> {
        PipelineLayoutBuilder {
            _phantom: PhantomData,
        }
    }

    pub fn with_push_constant<C: PushConstant>(self) -> PipelineLayoutBuilder<T, Cons<C, P>> {
        PipelineLayoutBuilder::<T, Cons<C, P>> {
            _phantom: PhantomData,
        }
    }

    pub fn with_descriptor_set<D: DescriptorLayout>(self) -> PipelineLayoutBuilder<Cons<D, T>, P> {
        PipelineLayoutBuilder::<Cons<D, T>, P> {
            _phantom: PhantomData,
        }
    }

    pub fn builder() -> Self {
        Self::default()
    }
}

impl<T: DescriptorLayoutList, P: PushConstantList> Layout for PipelineLayoutBuilder<T, P> {
    type Descriptors = T;
    type PushConstants = P;
}

#[derive(Debug, Clone, Copy)]
pub struct PipelineLayoutRaw {
    layout: vk::PipelineLayout,
}

impl<L: Layout> From<PipelineLayout<L>> for PipelineLayoutRaw {
    fn from(layout: PipelineLayout<L>) -> Self {
        Self {
            layout: layout.layout,
        }
    }
}

impl<L: Layout> From<PipelineLayoutRaw> for PipelineLayout<L> {
    fn from(layout: PipelineLayoutRaw) -> Self {
        Self {
            layout: layout.layout,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PipelineLayout<L: Layout> {
    layout: vk::PipelineLayout,
    _phantom: PhantomData<L>,
}

impl<L: Layout> PipelineLayout<L> {
    #[inline]
    pub unsafe fn wrap(layout: vk::PipelineLayout) -> Self {
        Self {
            layout,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn get_vk_layout(&self) -> vk::PipelineLayout {
        self.layout
    }
}

impl<L: Layout> From<PipelineLayout<L>> for vk::PipelineLayout {
    fn from(layout: PipelineLayout<L>) -> Self {
        layout.layout
    }
}

impl<L: Layout> TypeUniqueResource for PipelineLayout<L> {
    type RawType = PipelineLayoutRaw;
}

impl<L: Layout> FromGuard for PipelineLayout<L> {
    type Inner = PipelineLayoutRaw;

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

impl<L: Layout> Create for PipelineLayout<L> {
    type Config<'a> = ();

    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(
        _config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let push_ranges = PushConstantRanges::<L::PushConstants>::get_ranges();
        let layout = unsafe {
            context.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::builder()
                    .push_constant_ranges(&push_ranges)
                    .set_layouts(&context.get_descriptor_layouts::<L::Descriptors>()?),
                None,
            )?
        };
        Ok(Self {
            layout,
            _phantom: PhantomData,
        })
    }
}

impl<L: Layout> Destroy for PipelineLayout<L> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        unsafe {
            context.destroy_pipeline_layout(self.layout, None);
        }
        Ok(())
    }
}

impl Destroy for PipelineLayoutRaw {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        unsafe {
            context.destroy_pipeline_layout(self.layout, None);
        }
        Ok(())
    }
}

impl Context {
    fn get_descriptor_list_entry<'a, T: DescriptorLayoutList>(
        &self,
        mut iter: impl Iterator<Item = &'a mut vk::DescriptorSetLayout>,
    ) -> ResourceResult<()> {
        if !T::exhausted() {
            if let Some(entry) = iter.next() {
                *entry = self
                    .get_or_create_unique_resource::<DescriptorSetLayout<T::Item>, _>()?
                    .layout;
            }
            self.get_descriptor_list_entry::<T::Next>(iter)
        } else {
            Ok(())
        }
    }

    fn get_descriptor_layouts<T: DescriptorLayoutList>(
        &self,
    ) -> ResourceResult<Vec<vk::DescriptorSetLayout>> {
        let mut layouts = vec![vk::DescriptorSetLayout::null(); T::len()];
        self.get_descriptor_list_entry::<T>(layouts.iter_mut().rev())?;
        Ok(layouts)
    }
}
