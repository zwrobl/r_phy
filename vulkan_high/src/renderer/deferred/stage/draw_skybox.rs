use std::convert::Infallible;

use type_kit::{
    Cons, Create, Dependency, Destroy, MutList, Nil, Task, TypedNil, dependency_list, list_type,
    unpack_list,
};
use vulkan_low::{
    Context, index_list,
    memory::allocator::AllocatorIndex,
    resources::{
        command::{Graphics, NextRenderPass, PersistentCommandPool, Secondary},
        error::ResourceError,
        render_pass::RenderPass,
        storage::ResourceIndexListBuilder,
    },
};

use crate::{
    renderer::{
        DestroyTerminator, ExternalResources, ResourceCell,
        deferred::{
            presets::{
                AttachmentsGBuffer, DeferedRenderPass, GBufferSkyboxPass, GBufferSkyboxPipeline,
            },
            stage::depth_prepass::DepthPrepass,
        },
        frame::FrameCell,
    },
    resources::{Skybox, SkyboxPartial},
};

pub struct DrawSkybox {
    skybox: Skybox<GBufferSkyboxPipeline<AttachmentsGBuffer>>,
}

impl Create for DrawSkybox {
    type Config<'a> = (SkyboxPartial, Option<AllocatorIndex>);
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(
        config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let skybox = Skybox::create(config, context)?;
        Ok(Self { skybox })
    }
}

impl Destroy for DrawSkybox {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        let _ = self.skybox.destroy(context);
        Ok(())
    }
}

unsafe impl Task for DrawSkybox {
    type Dependencies = dependency_list![DepthPrepass];
    type ResourceSet = list_type![
        ExternalResources,
        ResourceCell<PersistentCommandPool<Secondary, Graphics>>,
        FrameCell<DeferedRenderPass<AttachmentsGBuffer>>,
        TypedNil<DestroyTerminator>
    ];
    type InitializerList = Nil;
    type TaskError = ResourceError;
    type TaskResult = ();

    fn execute<'a>(
        &'a mut self,
        unpack_list![context, command_pool, frame]: MutList<'a, Self::ResourceSet>,
    ) -> Result<Self::TaskResult, Self::TaskError> {
        let common_resources = &context.common_resources();
        let render_pass = context
            .get_unique_resource::<RenderPass<DeferedRenderPass<AttachmentsGBuffer>>, _>()?;
        let command = context.operate_mut(
            index_list![command_pool.index()],
            |unpack_list![command_pool]| {
                let command = context.begin_secondary_command::<_, _, _, GBufferSkyboxPass<_>>(
                    command_pool.next_command().1,
                    render_pass,
                    frame.swapchain_frame.framebuffer,
                )?;
                let command = context
                    .start_recording(command)
                    .push(&self.skybox.draw(common_resources, frame.camera_matrices))
                    .stop_recording();
                Result::<_, ResourceError>::Ok(context.finish_command(command)?)
            },
        )??;
        frame.primary_command = Some(
            context
                .start_recording(frame.primary_command.take().unwrap())
                .push(&command)
                .push(&NextRenderPass)
                .stop_recording(),
        );
        Ok(())
    }
}
