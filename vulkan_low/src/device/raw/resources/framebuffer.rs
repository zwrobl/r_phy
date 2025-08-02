pub mod presets;

use std::{convert::Infallible, fmt::Debug, marker::PhantomData, ops::Deref, ptr::NonNull, usize};

use ash::vk;

use crate::{
    device::{
        raw::resources::{
            image::{DescriptorImageInfo, Image, Image2D, ImageType, ImageView},
            render_pass::{RenderPass, RenderPassConfig},
            Resource,
        },
        AttachmentProperties,
    },
    error::ResourceError,
    Context,
};
use type_kit::{Cons, Create, Destroy, FromGuard, Nil, TypeGuardCollection};

use crate::device::memory::DeviceLocal;

pub trait ClearValue {
    fn get(&self) -> Option<vk::ClearValue>;
}

#[derive(Debug, Clone, Copy)]
pub struct ClearNone;

impl ClearValue for ClearNone {
    #[inline]
    fn get(&self) -> Option<vk::ClearValue> {
        None
    }
}

#[derive(Clone, Copy)]
pub struct ClearColor {
    color: vk::ClearColorValue,
}

impl Debug for ClearColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClearColor")
            .field("color", unsafe { &self.color.float32 })
            .finish()
    }
}

impl ClearColor {
    #[inline]
    pub fn new(color: [f32; 4]) -> Self {
        Self {
            color: vk::ClearColorValue { float32: color },
        }
    }
}

impl ClearValue for ClearColor {
    #[inline]
    fn get(&self) -> Option<vk::ClearValue> {
        Some(vk::ClearValue { color: self.color })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClearDeptStencil {
    depth_stencil: vk::ClearDepthStencilValue,
}

impl ClearDeptStencil {
    #[inline]
    pub fn new(depth: f32, stencil: u32) -> Self {
        Self {
            depth_stencil: vk::ClearDepthStencilValue { depth, stencil },
        }
    }
}

impl ClearValue for ClearDeptStencil {
    #[inline]
    fn get(&self) -> Option<vk::ClearValue> {
        Some(vk::ClearValue {
            depth_stencil: self.depth_stencil,
        })
    }
}

impl ClearValue for Nil {
    #[inline]
    fn get(&self) -> Option<vk::ClearValue> {
        unreachable!()
    }
}

fn write_clear_values<N: ClearValueList + ?Sized>(
    node: &N,
    mut vec: Vec<Option<vk::ClearValue>>,
) -> Vec<Option<vk::ClearValue>> {
    if N::LEN > 0 {
        vec.push(node.get());
        write_clear_values(node.next(), vec)
    } else {
        vec
    }
}

pub trait ClearValueList {
    const LEN: usize;
    type Item: ClearValue;
    type Next: ClearValueList;

    fn values(&self) -> Vec<Option<vk::ClearValue>> {
        write_clear_values(self, Vec::with_capacity(Self::LEN))
    }

    fn get(&self) -> Option<vk::ClearValue>;

    fn next(&self) -> &Self::Next;
}

impl ClearValueList for Nil {
    const LEN: usize = 0;
    type Item = Self;
    type Next = Self;

    fn get(&self) -> Option<vk::ClearValue> {
        unreachable!()
    }

    fn next(&self) -> &Self::Next {
        unreachable!()
    }
}

impl<C: ClearValue, N: ClearValueList> ClearValueList for Cons<C, N> {
    const LEN: usize = Self::Next::LEN + 1;
    type Item = C;
    type Next = N;

    fn get(&self) -> Option<vk::ClearValue> {
        self.head.get()
    }

    fn next(&self) -> &Self::Next {
        &self.tail
    }
}

pub struct ClearValueBuilder<C: ClearValueList> {
    clear_values: C,
}

impl Default for ClearValueBuilder<Nil> {
    fn default() -> Self {
        Self::new()
    }
}

impl ClearValueBuilder<Nil> {
    pub fn new() -> Self {
        Self {
            clear_values: Nil::new(),
        }
    }
}

impl<V: ClearValueList> ClearValueBuilder<V> {
    pub fn push<N: ClearValue>(self, value: N) -> ClearValueBuilder<Cons<N, V>> {
        let Self { clear_values } = self;
        ClearValueBuilder {
            clear_values: Cons {
                head: value,
                tail: clear_values,
            },
        }
    }

    pub fn get_clear_values(&self) -> Vec<vk::ClearValue> {
        self.clear_values.values().into_iter().flatten().collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttachmentUsage {
    Color,
    DepthStencil,
    Resolve,
    Input,
}

impl AttachmentUsage {
    #[inline]
    fn get_layout(&self) -> vk::ImageLayout {
        match self {
            Self::Color => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            Self::DepthStencil => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            Self::Resolve => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            Self::Input => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }
    }

    #[inline]
    pub fn get_usage_flags(&self) -> vk::ImageUsageFlags {
        match self {
            Self::Color => vk::ImageUsageFlags::COLOR_ATTACHMENT,
            Self::DepthStencil => vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            Self::Resolve => vk::ImageUsageFlags::COLOR_ATTACHMENT,
            Self::Input => vk::ImageUsageFlags::INPUT_ATTACHMENT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttachmentTarget {
    Use(AttachmentUsage),
    Preserve(AttachmentUsage),
}

#[derive(Debug, Clone, Copy)]
pub struct AttachmentReference {
    pub target: AttachmentTarget,
    pub layout: vk::ImageLayout,
    pub usage: vk::ImageUsageFlags,
}

impl Deref for AttachmentReference {
    type Target = AttachmentTarget;

    fn deref(&self) -> &Self::Target {
        &self.target
    }
}

impl AttachmentTarget {
    pub fn get_reference(&self) -> AttachmentReference {
        let usage = match *self {
            Self::Use(usage) => usage,
            Self::Preserve(usage) => usage,
        };
        AttachmentReference {
            target: *self,
            layout: usage.get_layout(),
            usage: usage.get_usage_flags(),
        }
    }

    pub fn try_get_usage(&self) -> Option<AttachmentUsage> {
        match *self {
            Self::Use(usage) => Some(usage),
            Self::Preserve(_) => None,
        }
    }

    pub fn get_sort_key(&self) -> usize {
        match *self {
            Self::Use(usage) => usage as usize,
            Self::Preserve(_) => usize::MAX,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IndexedAttachmentReference {
    pub reference: AttachmentReference,
    pub index: u32,
}

impl Deref for IndexedAttachmentReference {
    type Target = AttachmentReference;

    fn deref(&self) -> &Self::Target {
        &self.reference
    }
}

pub struct InputAttachment {
    pub image_view: vk::ImageView,
}

impl From<&Image<Image2D, DeviceLocal>> for InputAttachment {
    fn from(image: &Image<Image2D, DeviceLocal>) -> Self {
        Self {
            image_view: image.get_image_view().get_vk_image_view(),
        }
    }
}

impl From<&InputAttachment> for DescriptorImageInfo {
    fn from(attachment: &InputAttachment) -> Self {
        DescriptorImageInfo {
            image_info: vk::DescriptorImageInfo {
                image_view: attachment.image_view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                sampler: vk::Sampler::null(),
            },
        }
    }
}

pub trait AttachmentReferenceList {
    const LEN: usize;
    type Next: AttachmentReferenceList;

    fn values(&self, offset: usize) -> Vec<Option<IndexedAttachmentReference>>;

    fn next(&self) -> &Self::Next;

    fn get_value(&self) -> Option<AttachmentReference>;
}

impl AttachmentReferenceList for Nil {
    const LEN: usize = 0;
    type Next = Self;

    fn values(&self, _offset: usize) -> Vec<Option<IndexedAttachmentReference>> {
        vec![]
    }

    fn next(&self) -> &Self::Next {
        unreachable!()
    }

    fn get_value(&self) -> Option<AttachmentReference> {
        unreachable!()
    }
}

fn write_references<N: AttachmentReferenceList + ?Sized>(
    node: &N,
    offset: usize,
    mut vec: Vec<Option<IndexedAttachmentReference>>,
) -> Vec<Option<IndexedAttachmentReference>> {
    if N::LEN > 0 {
        vec.push(
            node.get_value()
                .map(|reference| IndexedAttachmentReference {
                    reference,
                    index: (N::LEN - 1 + offset) as u32,
                }),
        );
        write_references(node.next(), offset, vec)
    } else {
        vec
    }
}

impl<N: AttachmentReferenceList> AttachmentReferenceList for Cons<Option<AttachmentReference>, N> {
    const LEN: usize = Self::Next::LEN + 1;
    type Next = N;

    fn values(&self, offset: usize) -> Vec<Option<IndexedAttachmentReference>> {
        write_references(self, offset, Vec::with_capacity(Self::LEN))
    }

    fn next(&self) -> &Self::Next {
        &self.tail
    }

    fn get_value(&self) -> Option<AttachmentReference> {
        self.head
    }
}

pub struct AttachmentReferenceBuilder<A: AttachmentList> {
    pub references: A::ReferenceListType,
}

impl Default for AttachmentReferenceBuilder<Nil> {
    fn default() -> Self {
        Self::new()
    }
}

impl AttachmentReferenceBuilder<Nil> {
    pub fn new() -> Self {
        Self {
            references: Nil::new(),
        }
    }
}

impl<A: AttachmentList> AttachmentReferenceBuilder<A> {
    pub fn push<N: Attachment>(
        self,
        target: Option<AttachmentTarget>,
    ) -> AttachmentReferenceBuilder<Cons<AttachmentImage<N>, A>> {
        let Self { references } = self;
        AttachmentReferenceBuilder {
            references: Cons {
                head: target.map(|target| target.get_reference()),
                tail: references,
            },
        }
    }
}

pub trait AttachmentReferences {
    type Attachments: AttachmentList;

    fn get_references(&self) -> Vec<Option<IndexedAttachmentReference>>;
    fn get_input_attachments<C: RenderPassConfig<Attachments = Self::Attachments>>(
        &self,
        framebuffer: &Framebuffer<C>,
    ) -> Vec<InputAttachment>;
}

impl<A: AttachmentList> AttachmentReferences for AttachmentReferenceBuilder<A> {
    type Attachments = A;

    fn get_references(&self) -> Vec<Option<IndexedAttachmentReference>> {
        AttachmentReferenceList::values(&self.references, 0)
            .into_iter()
            .rev()
            .collect()
    }

    fn get_input_attachments<C: RenderPassConfig<Attachments = Self::Attachments>>(
        &self,
        framebuffer: &Framebuffer<C>,
    ) -> Vec<InputAttachment> {
        self.get_references()
            .into_iter()
            .zip(&framebuffer.attachments)
            .filter_map(|(reference, &attachment)| {
                if let Some(usage) = reference.map(|r| r.try_get_usage()).flatten() {
                    if usage == AttachmentUsage::Input {
                        return Some(InputAttachment {
                            image_view: attachment,
                        });
                    }
                }
                None
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LoadOp {
    Load,
    Clear,
    DontCare,
}

impl LoadOp {
    fn get_vk_load_op(self) -> vk::AttachmentLoadOp {
        match self {
            Self::Load => vk::AttachmentLoadOp::LOAD,
            Self::Clear => vk::AttachmentLoadOp::CLEAR,
            Self::DontCare => vk::AttachmentLoadOp::DONT_CARE,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StoreOp {
    Store,
    DontCare,
}

impl StoreOp {
    fn get_vk_store_op(self) -> vk::AttachmentStoreOp {
        match self {
            Self::Store => vk::AttachmentStoreOp::STORE,
            Self::DontCare => vk::AttachmentStoreOp::DONT_CARE,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ImageLayout {
    Undefined,
    ColorAttachment,
    DepthStencilAttachment,
    ShaderReadOnly,
    PresentSrc,
}

impl ImageLayout {
    fn get_vk_image_layout(self) -> vk::ImageLayout {
        match self {
            Self::Undefined => vk::ImageLayout::UNDEFINED,
            Self::ColorAttachment => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            Self::DepthStencilAttachment => vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            Self::ShaderReadOnly => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            Self::PresentSrc => vk::ImageLayout::PRESENT_SRC_KHR,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AttachmentTransition {
    pub(crate) load_op: vk::AttachmentLoadOp,
    pub(crate) store_op: vk::AttachmentStoreOp,
    pub(crate) initial_layout: vk::ImageLayout,
    pub(crate) final_layout: vk::ImageLayout,
}

impl AttachmentTransition {
    pub fn new(
        load_op: LoadOp,
        store_op: StoreOp,
        initial_layout: ImageLayout,
        final_layout: ImageLayout,
    ) -> Self {
        Self {
            load_op: load_op.get_vk_load_op(),
            store_op: store_op.get_vk_store_op(),
            initial_layout: initial_layout.get_vk_image_layout(),
            final_layout: final_layout.get_vk_image_layout(),
        }
    }
}

pub trait AttachmentTransitionList {
    const LEN: usize;
    type Next: AttachmentTransitionList;

    fn values(&self) -> Vec<AttachmentTransition>;

    fn next(&self) -> &Self::Next;

    fn get_value(&self) -> AttachmentTransition;
}

impl AttachmentTransitionList for Nil {
    const LEN: usize = 0;
    type Next = Self;

    fn values(&self) -> Vec<AttachmentTransition> {
        vec![]
    }

    fn next(&self) -> &Self::Next {
        unreachable!()
    }

    fn get_value(&self) -> AttachmentTransition {
        unreachable!()
    }
}

fn write_transitions<N: AttachmentTransitionList + ?Sized>(
    node: &N,
    mut vec: Vec<AttachmentTransition>,
) -> Vec<AttachmentTransition> {
    if N::LEN > 0 {
        vec.push(node.get_value());
        write_transitions(node.next(), vec)
    } else {
        vec
    }
}

impl<N: AttachmentTransitionList> AttachmentTransitionList for Cons<AttachmentTransition, N> {
    const LEN: usize = Self::Next::LEN + 1;
    type Next = N;

    fn values(&self) -> Vec<AttachmentTransition> {
        write_transitions(self, Vec::with_capacity(Self::LEN))
    }

    fn next(&self) -> &Self::Next {
        &self.tail
    }

    fn get_value(&self) -> AttachmentTransition {
        self.head
    }
}

pub struct AttachmentTransitionBuilder<A: AttachmentTransitionList> {
    transitions: A,
}

impl Default for AttachmentTransitionBuilder<Nil> {
    fn default() -> Self {
        Self::new()
    }
}

impl AttachmentTransitionBuilder<Nil> {
    pub fn new() -> Self {
        Self {
            transitions: Nil::new(),
        }
    }
}

impl<A: AttachmentTransitionList> AttachmentTransitionBuilder<A> {
    pub fn push(
        self,
        transition: AttachmentTransition,
    ) -> AttachmentTransitionBuilder<Cons<AttachmentTransition, A>> {
        let Self { transitions } = self;
        AttachmentTransitionBuilder {
            transitions: Cons {
                head: transition,
                tail: transitions,
            },
        }
    }
}

pub trait AttachmentTransistions {
    fn get(&self) -> Vec<AttachmentTransition>;
}

impl<A: AttachmentTransitionList> AttachmentTransistions for AttachmentTransitionBuilder<A> {
    fn get(&self) -> Vec<AttachmentTransition> {
        self.transitions.values().into_iter().rev().collect()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AttachmentFormatInfo {
    pub format: vk::Format,
    pub samples: vk::SampleCountFlags,
}

pub trait Attachment: 'static {
    type Clear: ClearValue;

    fn get_format(properties: &AttachmentProperties) -> AttachmentFormatInfo;
}

fn write_image_views<N: AttachmentList + ?Sized>(
    node: &N,
    mut vec: Vec<vk::ImageView>,
) -> Vec<vk::ImageView> {
    if N::LEN > 0 {
        vec.push(node.view());
        write_image_views(node.next(), vec)
    } else {
        vec
    }
}

pub trait AttachmentList: 'static {
    const LEN: usize;
    type Item: Attachment;
    type Next: AttachmentList;
    type ClearListType: ClearValueList;
    type ReferenceListType: AttachmentReferenceList;
    type TransitionListType: AttachmentTransitionList;

    fn values(&self) -> Vec<vk::ImageView> {
        write_image_views(self, Vec::with_capacity(Self::LEN))
    }

    fn next(&self) -> &Self::Next;

    fn view(&self) -> vk::ImageView;
}

fn write_formats<N: AttachmentList + ?Sized>(
    properties: &AttachmentProperties,
    mut vec: Vec<AttachmentFormatInfo>,
) -> Vec<AttachmentFormatInfo> {
    if N::LEN > 0 {
        vec.push(N::Item::get_format(properties));
        write_formats::<N::Next>(properties, vec)
    } else {
        vec
    }
}

pub trait AttachmentListFormats: AttachmentList {
    fn values(properties: &AttachmentProperties) -> Vec<AttachmentFormatInfo> {
        write_formats::<Self>(properties, Vec::with_capacity(Self::LEN))
            .into_iter()
            .rev()
            .collect()
    }
}

impl<T: AttachmentList> AttachmentListFormats for T {}

impl Attachment for Nil {
    type Clear = ClearNone;

    fn get_format(_properties: &AttachmentProperties) -> AttachmentFormatInfo {
        panic!("get_format called on Nil!");
    }
}

impl AttachmentList for Nil {
    const LEN: usize = 0;
    type Item = Self;
    type Next = Self;
    type ClearListType = Nil;
    type ReferenceListType = Nil;
    type TransitionListType = Nil;

    fn next(&self) -> &Self::Next {
        unreachable!()
    }

    fn view(&self) -> vk::ImageView {
        unreachable!()
    }
}

pub struct AttachmentImage<A: Attachment> {
    view: vk::ImageView,
    _phantom: PhantomData<A>,
}

impl<A: Attachment, N: AttachmentList> AttachmentList for Cons<AttachmentImage<A>, N> {
    const LEN: usize = N::LEN + 1;
    type Item = A;
    type Next = N;
    type ClearListType = Cons<A::Clear, N::ClearListType>;
    type ReferenceListType = Cons<Option<AttachmentReference>, N::ReferenceListType>;
    type TransitionListType = Cons<AttachmentTransition, N::TransitionListType>;

    fn next(&self) -> &Self::Next {
        &self.tail
    }

    fn view(&self) -> vk::ImageView {
        self.head.view
    }
}

#[derive(Debug)]
pub struct AttachmentsBuilder<A: AttachmentList> {
    attachments: A,
}

impl Default for AttachmentsBuilder<Nil> {
    fn default() -> Self {
        Self::new()
    }
}

impl AttachmentsBuilder<Nil> {
    pub fn new() -> Self {
        Self {
            attachments: Nil::new(),
        }
    }
}

impl<A: AttachmentList> AttachmentsBuilder<A> {
    pub fn push<N: Attachment, V: ImageType>(
        self,
        view: &ImageView<V>,
    ) -> AttachmentsBuilder<Cons<AttachmentImage<N>, A>> {
        let Self { attachments } = self;
        AttachmentsBuilder {
            attachments: Cons {
                head: AttachmentImage {
                    view: view.get_vk_image_view(),
                    _phantom: PhantomData,
                },

                tail: attachments,
            },
        }
    }

    pub fn get_attachments(&self) -> Vec<vk::ImageView> {
        self.attachments.values().into_iter().collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Extent2D {
    extent: vk::Extent2D,
}

impl Extent2D {
    #[inline]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            extent: vk::Extent2D { width, height },
        }
    }
}

#[derive(Debug)]
pub struct FramebufferBuilder<C: RenderPassConfig> {
    extent: vk::Extent2D,
    attachments: AttachmentsBuilder<C::Attachments>,
}

impl<C: RenderPassConfig> FramebufferBuilder<C> {
    pub fn new(extent: Extent2D, attachments: AttachmentsBuilder<C::Attachments>) -> Self {
        Self {
            extent: extent.extent,
            attachments,
        }
    }
}

pub type References<A> = AttachmentReferenceBuilder<A>;

pub type Transitions<A> = AttachmentTransitionBuilder<<A as AttachmentList>::TransitionListType>;

pub type Clear<A> = ClearValueBuilder<<A as AttachmentList>::ClearListType>;

#[derive(Debug, Clone, Copy)]
pub struct FramebufferRaw {
    pub framebuffer: vk::Framebuffer,
    pub attachments: Option<NonNull<[vk::ImageView]>>,
}

#[derive(Debug)]
pub struct Framebuffer<C: RenderPassConfig> {
    pub framebuffer: vk::Framebuffer,
    pub attachments: Box<[vk::ImageView]>,
    _phantom: PhantomData<C>,
}

#[derive(Debug)]
pub struct FramebufferHandle<C: RenderPassConfig> {
    pub framebuffer: vk::Framebuffer,
    _phantom: PhantomData<C>,
}

impl<C: RenderPassConfig> Clone for FramebufferHandle<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: RenderPassConfig> From<&Framebuffer<C>> for FramebufferHandle<C> {
    fn from(framebuffer: &Framebuffer<C>) -> Self {
        Self {
            framebuffer: framebuffer.framebuffer,
            _phantom: PhantomData,
        }
    }
}

impl<C: RenderPassConfig> Copy for FramebufferHandle<C> {}

impl<C: RenderPassConfig> Resource for Framebuffer<C> {
    type RawType = FramebufferRaw;
    type RawCollection = TypeGuardCollection<Self::RawType>;
}

impl<C: RenderPassConfig> FromGuard for Framebuffer<C> {
    type Inner = FramebufferRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner {
            framebuffer: self.framebuffer,
            attachments: NonNull::new(Box::leak(self.attachments)),
        }
    }

    #[inline]
    unsafe fn from_inner(mut inner: Self::Inner) -> Self {
        Self {
            framebuffer: inner.framebuffer,
            attachments: unsafe { Box::from_raw(inner.attachments.take().unwrap().as_mut()) },
            _phantom: PhantomData,
        }
    }
}

impl<C: RenderPassConfig> Create for Framebuffer<C> {
    type Config<'a> = FramebufferBuilder<C>;

    type CreateError = ResourceError;

    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let FramebufferBuilder {
            extent,
            attachments,
        } = config;
        let render_pass = context.get_or_create_unique_resource::<RenderPass<C>, _>()?;
        let attachments = attachments.get_attachments().into_boxed_slice();
        let create_info = vk::FramebufferCreateInfo::builder()
            .attachments(&attachments)
            .render_pass(render_pass.handle)
            .width(extent.width)
            .height(extent.height)
            .layers(1);
        let framebuffer = unsafe { context.create_framebuffer(&create_info, None)? };
        Ok(Framebuffer {
            framebuffer,
            attachments,
            _phantom: PhantomData,
        })
    }
}

impl<C: RenderPassConfig> Destroy for Framebuffer<C> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        unsafe {
            context.destroy_framebuffer(self.framebuffer, None);
        }
        Ok(())
    }
}

impl Destroy for FramebufferRaw {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        unsafe {
            context.destroy_framebuffer(self.framebuffer, None);
        }
        self.attachments
            .take()
            .map(|mut attachments| drop(unsafe { Box::from_raw(attachments.as_mut()) }));
        Ok(())
    }
}
