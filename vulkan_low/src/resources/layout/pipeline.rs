use std::{any::TypeId, convert::Infallible, marker::PhantomData};

use ash::vk;

use crate::{
    Context,
    resources::{
        descriptor::DescriptorWriteInfo,
        error::{ResourceError, ResourceResult},
        layout::{
            DescriptorBinding, DescriptorLayout, DescriptorSetLayout, DescriptorSetLayoutBinding,
        },
        storage::TypeUniqueResource,
    },
};
use type_kit::{Cons, Contains, Create, Destroy, FromGuard, Marker, Nil};

pub struct PushRange<P: PushConstant> {
    range: vk::PushConstantRange,
    _phantom: PhantomData<P>,
}

impl<P: PushConstant> PushRange<P> {
    #[inline]
    pub fn new(range: vk::PushConstantRange) -> Self {
        Self {
            range,
            _phantom: PhantomData,
        }
    }
}

pub trait Layout: 'static {
    type Descriptors: DescriptorLayoutList;
    type PushConstants: PushConstantList;

    fn ranges() -> PushConstantRanges<Self::PushConstants> {
        PushConstantRanges::<Self::PushConstants>::new()
    }

    fn sets() -> DescriptorSets<Self::Descriptors> {
        DescriptorSets::<Self::Descriptors>::new()
    }
}

pub trait PushConstant: 'static {
    fn range(offset: u32) -> vk::PushConstantRange;
}

pub trait PushConstantList: 'static {
    type Item: PushConstant;
    type Next: PushConstantList;
    type RangeList;

    fn exhausted() -> bool;
    fn len() -> usize;
    fn offset_list(offset: u32) -> Self::RangeList;
}

impl PushConstant for Nil {
    fn range(_offset: u32) -> vk::PushConstantRange {
        unreachable!()
    }
}

impl PushConstantList for Nil {
    type Item = Self;
    type Next = Self;
    type RangeList = Nil;

    fn exhausted() -> bool {
        true
    }

    fn len() -> usize {
        0
    }

    fn offset_list(_offset: u32) -> Self::RangeList {
        Nil::new()
    }
}

impl<P: PushConstant, N: PushConstantList> PushConstantList for Cons<P, N> {
    type Item = P;
    type Next = N;
    type RangeList = Cons<PushRange<P>, N::RangeList>;

    fn exhausted() -> bool {
        false
    }

    fn len() -> usize {
        Self::Next::len() + 1
    }

    fn offset_list(offset: u32) -> Self::RangeList {
        let range = Self::Item::range(offset);
        Cons::new(PushRange::new(range), N::offset_list(offset + range.size))
    }
}

pub struct PushConstantRanges<N: PushConstantList> {
    ranges: N::RangeList,
}

impl<N: PushConstantList> Default for PushConstantRanges<N> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<N: PushConstantList> PushConstantRanges<N> {
    #[inline]
    pub fn new() -> Self {
        Self {
            ranges: N::offset_list(0),
        }
    }

    #[inline]
    pub fn get_range<P: PushConstant, M: Marker>(&self) -> vk::PushConstantRange
    where
        N::RangeList: Contains<PushRange<P>, M>,
    {
        self.ranges.get().range
    }

    fn next_push_range<'a, T: PushConstantList>(
        offset: u32,
        mut iter: impl Iterator<Item = &'a mut vk::PushConstantRange>,
    ) {
        if !T::exhausted()
            && let Some(entry) = iter.next()
        {
            let range = T::Item::range(offset);
            *entry = range;
            Self::next_push_range::<T::Next>(offset + range.size, iter)
        }
    }

    fn get_ranges() -> Vec<vk::PushConstantRange> {
        let mut ranges = vec![vk::PushConstantRange::default(); N::len()];
        Self::next_push_range::<N>(0, ranges.iter_mut());
        ranges
    }
}

pub struct DescriptorIndex<T: DescriptorLayout> {
    pub index: u32,
    _phantom: PhantomData<T>,
}

impl<T: DescriptorLayout> Clone for DescriptorIndex<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: DescriptorLayout> Copy for DescriptorIndex<T> {}

impl<T: DescriptorLayout> DescriptorIndex<T> {
    #[inline]
    pub fn new(index: u32) -> Self {
        Self {
            index,
            _phantom: PhantomData,
        }
    }
}

pub trait DescriptorLayoutList: 'static {
    type Item: DescriptorLayout;
    type Next: DescriptorLayoutList;
    type IndexList;

    fn exhausted() -> bool;
    fn len() -> usize;
    fn index_list(base_index: u32) -> Self::IndexList;
}

impl DescriptorLayoutList for Nil {
    type Item = Self;
    type Next = Self;
    type IndexList = Nil;

    fn exhausted() -> bool {
        true
    }

    fn len() -> usize {
        0
    }

    fn index_list(_base_index: u32) -> Self::IndexList {
        Nil::new()
    }
}

impl DescriptorLayout for Nil {
    fn get_descriptor_set_bindings() -> Vec<DescriptorSetLayoutBinding> {
        unreachable!()
    }

    fn get_descriptor_pools(_num_sets: u32) -> Vec<vk::DescriptorPoolSize> {
        unreachable!()
    }

    fn get_descriptor_writes<T: DescriptorBinding>() -> Vec<DescriptorWriteInfo> {
        unreachable!()
    }
}

impl<L: DescriptorLayout, N: DescriptorLayoutList> DescriptorLayoutList for Cons<L, N> {
    type Item = L;
    type Next = N;
    type IndexList = Cons<DescriptorIndex<Self::Item>, N::IndexList>;

    fn exhausted() -> bool {
        false
    }

    fn len() -> usize {
        Self::Next::len() + 1
    }

    fn index_list(base_index: u32) -> Self::IndexList {
        Cons::new(
            DescriptorIndex::new(base_index),
            N::index_list(base_index.saturating_sub(1)),
        )
    }
}

pub struct DescriptorSets<L: DescriptorLayoutList> {
    indices: L::IndexList,
}

impl<L: DescriptorLayoutList> Default for DescriptorSets<L> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L: DescriptorLayoutList> DescriptorSets<L> {
    pub fn new() -> DescriptorSets<L> {
        DescriptorSets {
            // TODO: Reverse list order to avoid subtraction here
            indices: L::index_list(L::len().saturating_sub(1) as u32),
        }
    }

    pub fn get_index<T: DescriptorLayout, M: Marker>(&self) -> DescriptorIndex<T>
    where
        L::IndexList: Contains<DescriptorIndex<T>, M>,
    {
        *self.indices.get()
    }

    fn try_get_index_impl<T: DescriptorLayout, N: DescriptorLayoutList>(index: u32) -> Option<u32> {
        if !N::exhausted() {
            if TypeId::of::<T>() == TypeId::of::<N::Item>() {
                Some(index - 1)
            } else {
                Self::try_get_index_impl::<T, N::Next>(index - 1)
            }
        } else {
            None
        }
    }

    pub fn try_get_index<T: DescriptorLayout>(&self) -> Option<u32> {
        Self::try_get_index_impl::<T, L>(L::len() as u32)
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
    /// # Safety
    /// This allows user to create a type-safe PipelineLayout instance from a raw Vulkan pipeline layout handle.
    /// The caller must ensure that the raw handle is valid and corresponds to a PipelineLayout instance.
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
                    .get_unique_resource::<DescriptorSetLayout<T::Item>, _>()?
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
