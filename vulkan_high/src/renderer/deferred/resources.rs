use std::convert::Infallible;

use type_kit::{Cons, Create, CreateResult, Destroy, DestroyResult, DropGuard, unpack_list};
use vulkan_low::{
    Context, index_list,
    memory::{
        DeviceLocal,
        allocator::{AllocatorBuilder, AllocatorIndex},
    },
    resources::{
        Partial, ResourceIndex,
        descriptor::{DescriptorPool, DescriptorSetWriter},
        error::ResourceError,
        framebuffer::{
            AttachmentReferences, AttachmentsBuilder, Extent2D, Framebuffer, FramebufferBuilder,
            InputAttachment,
        },
        image::{Image, Image2D, ImagePartial, ImageView},
        render_pass::Subpass,
        storage::ResourceIndexListBuilder,
        swapchain::SwapchainFramebufferConfigBuilder,
    },
};

use crate::renderer::deferred::presets::{
    AttachmentsGBuffer, DeferedRenderPass, GBufferDescriptorSet, GBufferShadingPass,
};

pub struct GBufferPartial {
    pub albedo: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
    pub normal: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
    pub position: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
    pub combined: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
    pub depth: DropGuard<ImagePartial<Image2D, DeviceLocal>>,
}

pub struct GBuffer {
    pub albedo: ResourceIndex<Image<Image2D, DeviceLocal>>,
    pub normal: ResourceIndex<Image<Image2D, DeviceLocal>>,
    pub position: ResourceIndex<Image<Image2D, DeviceLocal>>,
    pub combined: ResourceIndex<Image<Image2D, DeviceLocal>>,
    pub depth: ResourceIndex<Image<Image2D, DeviceLocal>>,
}

impl Create for GBufferPartial {
    type Config<'a> = ();

    type CreateError = ResourceError;

    fn create<'a, 'b>(_config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let albedo = DropGuard::new(context.prepare_color_attachment_image()?);
        let normal = DropGuard::new(context.prepare_color_attachment_image()?);
        let position = DropGuard::new(context.prepare_color_attachment_image()?);
        let combined = DropGuard::new(context.prepare_color_attachment_image()?);
        let depth = DropGuard::new(context.prepare_depth_stencil_attachment_image()?);
        Ok(GBufferPartial {
            albedo,
            normal,
            position,
            combined,
            depth,
        })
    }
}

impl Partial for GBufferPartial {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.albedo.register_memory_requirements(builder);
        self.normal.register_memory_requirements(builder);
        self.position.register_memory_requirements(builder);
        self.combined.register_memory_requirements(builder);
        self.depth.register_memory_requirements(builder);
    }
}

impl Destroy for GBufferPartial {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.albedo.destroy(context);
        let _ = self.normal.destroy(context);
        let _ = self.position.destroy(context);
        let _ = self.combined.destroy(context);
        let _ = self.depth.destroy(context);
        Ok(())
    }
}

impl SwapchainFramebufferConfigBuilder<DeferedRenderPass<AttachmentsGBuffer>> for GBuffer {
    fn get_framebuffer_builder(
        &self,
        context: &Context,
        swapchain_image: &ImageView<Image2D>,
        extent: Extent2D,
    ) -> FramebufferBuilder<DeferedRenderPass<AttachmentsGBuffer>> {
        context
            .operate_ref(
                index_list![
                    self.combined,
                    self.albedo,
                    self.normal,
                    self.position,
                    self.depth
                ],
                |unpack_list![depth, position, normal, albedo, combined]| {
                    FramebufferBuilder::new(
                        extent,
                        AttachmentsBuilder::new()
                            .push(swapchain_image)
                            .push(depth.get_image_view())
                            .push(position.get_image_view())
                            .push(normal.get_image_view())
                            .push(albedo.get_image_view())
                            .push(combined.get_image_view()),
                    )
                },
            )
            .unwrap()
    }
}

impl Create for GBuffer {
    type Config<'a> = (GBufferPartial, Option<AllocatorIndex>);
    type CreateError = ResourceError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (partial, allocator) = config;
        let albedo = context.create_resource::<Image<_, _>, _>((partial.albedo, allocator))?;
        let normal = context.create_resource::<Image<_, _>, _>((partial.normal, allocator))?;
        let position = context.create_resource::<Image<_, _>, _>((partial.position, allocator))?;
        let combined = context.create_resource::<Image<_, _>, _>((partial.combined, allocator))?;
        let depth = context.create_resource::<Image<_, _>, _>((partial.depth, allocator))?;
        Ok(GBuffer {
            combined,
            albedo,
            normal,
            position,
            depth,
        })
    }
}

impl Destroy for GBuffer {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = context.destroy_resource(self.albedo);
        let _ = context.destroy_resource(self.normal);
        let _ = context.destroy_resource(self.position);
        let _ = context.destroy_resource(self.combined);
        let _ = context.destroy_resource(self.depth);
        Ok(())
    }
}

pub struct DeferredSharedResources {
    pub descriptor_pool: ResourceIndex<DescriptorPool<GBufferDescriptorSet>>,
}

impl Create for DeferredSharedResources {
    type Config<'a> = ResourceIndex<Framebuffer<DeferedRenderPass<AttachmentsGBuffer>>>;
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let descriptor_pool = {
            context.operate_ref(index_list![config], |unpack_list![framebuffer]| {
                context.create_resource::<DescriptorPool<_>, _>(
                    DescriptorSetWriter::<GBufferDescriptorSet>::new(1)
                        .write_images::<InputAttachment>(
                            &GBufferShadingPass::<AttachmentsGBuffer>::references()
                                .get_input_attachments(framebuffer)
                                .iter()
                                .map(|attachment| attachment.into())
                                .collect::<Vec<_>>(),
                        ),
                )
            })??
        };
        Ok(Self { descriptor_pool })
    }
}

impl Destroy for DeferredSharedResources {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, context: &Context) -> Result<(), Self::DestroyError> {
        let _ = context.destroy_resource(self.descriptor_pool);
        Ok(())
    }
}
