use std::{any::TypeId, collections::HashMap, convert::Infallible, marker::PhantomData};

use ash::vk;

use crate::{
    error::ResourceError,
    resources::{descriptor::DescriptorWriteInfo, storage::TypeUniqueResource},
    Context,
};
use type_kit::{Cons, Create, Destroy, FromGuard, Nil};

#[derive(Debug, Clone, Copy)]
pub enum ShaderStage {
    Vertex,
    Fragment,
}

impl ShaderStage {
    pub fn get_vk_stage_flags(self) -> vk::ShaderStageFlags {
        match self {
            Self::Vertex => vk::ShaderStageFlags::VERTEX,
            Self::Fragment => vk::ShaderStageFlags::FRAGMENT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DescriptorType {
    CombinedImageSampler,
    UniformBuffer,
    InputAttachment,
}

impl DescriptorType {
    #[inline]
    pub fn pool_size(self, count: u32) -> DescriptorPoolSize {
        DescriptorPoolSize::new(self, count)
    }

    #[inline]
    pub fn write_info(self, binding: u32) -> DescriptorWriteInfo {
        DescriptorWriteInfo::new(binding, self)
    }

    #[inline]
    pub fn layout_binding(self, binding: u32) -> DescriptorSetLayoutBinding {
        DescriptorSetLayoutBinding::new(self, binding)
    }

    pub(crate) fn get_vk_descriptor_type(self) -> vk::DescriptorType {
        match self {
            Self::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            Self::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
            Self::InputAttachment => vk::DescriptorType::INPUT_ATTACHMENT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorPoolSize {
    ty: vk::DescriptorType,
    count: u32,
}

impl DescriptorPoolSize {
    #[inline]
    pub fn new(ty: DescriptorType, count: u32) -> Self {
        Self {
            ty: ty.get_vk_descriptor_type(),
            count,
        }
    }

    #[inline]
    pub fn get_vk_descriptor_pool_size(self) -> vk::DescriptorPoolSize {
        vk::DescriptorPoolSize {
            ty: self.ty,
            descriptor_count: self.count,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorSetLayoutBinding {
    binding: u32,
    count: u32,
    ty: vk::DescriptorType,
    stage_flags: vk::ShaderStageFlags,
}

impl DescriptorSetLayoutBinding {
    #[inline]
    pub fn new(ty: DescriptorType, binding: u32) -> Self {
        Self {
            ty: ty.get_vk_descriptor_type(),
            binding,
            count: 1,
            stage_flags: vk::ShaderStageFlags::empty(),
        }
    }

    #[inline]
    pub fn with_shader_stage(self, stage: ShaderStage) -> Self {
        Self {
            stage_flags: self.stage_flags | stage.get_vk_stage_flags(),
            ..self
        }
    }

    #[inline]
    pub fn with_descriptor_count(self, count: u32) -> Self {
        Self { count, ..self }
    }

    fn get_vk_descriptor_set_layout_binding(self) -> vk::DescriptorSetLayoutBinding {
        vk::DescriptorSetLayoutBinding {
            binding: self.binding,
            descriptor_type: self.ty,
            descriptor_count: self.count,
            stage_flags: self.stage_flags,
            ..Default::default()
        }
    }
}

pub trait DescriptorBinding: 'static {
    fn has_data() -> bool;

    fn get_descriptor_set_binding(binding: u32) -> DescriptorSetLayoutBinding;

    fn get_descriptor_write(binding: u32) -> DescriptorWriteInfo;

    fn get_descriptor_pool(num_sets: u32) -> DescriptorPoolSize;
}

pub trait DescriptorLayout: 'static {
    fn get_descriptor_set_bindings() -> Vec<DescriptorSetLayoutBinding>;

    fn get_descriptor_writes<T: DescriptorBinding>() -> Vec<DescriptorWriteInfo>;

    fn get_descriptor_pools(num_sets: u32) -> Vec<vk::DescriptorPoolSize>;
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

    fn get_descriptor_set_binding(_binding: u32) -> DescriptorSetLayoutBinding {
        unreachable!()
    }

    fn get_descriptor_write(_binding: u32) -> DescriptorWriteInfo {
        unreachable!()
    }

    fn get_descriptor_pool(_num_sets: u32) -> DescriptorPoolSize {
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

    fn next_descriptor_binding<T: DescriptorBindingList>(
        binding: u32,
        mut descriptor_bindings: Vec<DescriptorSetLayoutBinding>,
    ) -> Vec<DescriptorSetLayoutBinding> {
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

    pub fn get_descriptor_bindings() -> Vec<DescriptorSetLayoutBinding> {
        Self::next_descriptor_binding::<B>(0, Vec::with_capacity(B::LEN))
    }

    fn try_get_descriptor_writes<S: DescriptorBinding, T: DescriptorBindingList>(
        binding: u32,
        mut vec: Vec<DescriptorWriteInfo>,
    ) -> Vec<DescriptorWriteInfo> {
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

    pub fn get_descriptor_writes<T: DescriptorBinding>() -> Vec<DescriptorWriteInfo> {
        Self::try_get_descriptor_writes::<T, B>(0, Vec::new())
    }

    fn next_descriptor_pool_size<T: DescriptorBindingList>(
        num_sets: u32,
        pool_sizes: &mut HashMap<vk::DescriptorType, u32>,
    ) {
        if T::LEN > 0 {
            if T::Item::has_data() {
                let pool_size =
                    T::Item::get_descriptor_pool(num_sets).get_vk_descriptor_pool_size();
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
    fn get_descriptor_set_bindings() -> Vec<DescriptorSetLayoutBinding> {
        Self::get_descriptor_bindings()
    }

    fn get_descriptor_writes<T: DescriptorBinding>() -> Vec<DescriptorWriteInfo> {
        Self::get_descriptor_writes::<T>()
    }

    fn get_descriptor_pools(num_sets: u32) -> Vec<vk::DescriptorPoolSize> {
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
        let bindings = T::get_descriptor_set_bindings()
            .into_iter()
            .map(|binding| binding.get_vk_descriptor_set_layout_binding())
            .collect::<Vec<_>>();
        let layout = unsafe {
            context.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings),
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
