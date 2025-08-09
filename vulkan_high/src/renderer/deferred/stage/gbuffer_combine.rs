use std::{convert::Infallible, path::Path};

use type_kit::{
    dependency_list, list_type, unpack_list, Cons, Create, Dependency, Destroy, ListMutType, Nil,
    Task, TypedNil,
};
use vulkan_low::{
    index_list,
    resources::{
        command::{EndRenderPass, Graphics, PersistentCommandPool, Secondary},
        error::ResourceError,
        pipeline::{GraphicsPipeline, ModuleLoader, ShaderDirectory},
        render_pass::RenderPass,
        storage::ResourceIndexListBuilder,
        ResourceIndex,
    },
    Context,
};

use crate::{
    renderer::{
        deferred::{
            presets::{
                AttachmentsGBuffer, DeferedRenderPass, GBufferShadingPass,
                GBufferShadingPassPipeline,
            },
            stage::gbuffer_write::GBufferWrite,
            DeferredSharedResources,
        },
        frame::{Frame, FrameCell},
        DestroyTerminator, ExternalResources, ResourceCell,
    },
    resources::CommonMesh,
};

const GBUFFER_COMBINE_SHADER: &str = "_resources/shaders/spv/deferred/gbuffer_combine";
pub struct GBufferCombine {
    shading_pass: ResourceIndex<GraphicsPipeline<GBufferShadingPassPipeline<AttachmentsGBuffer>>>,
}

impl Create for GBufferCombine {
    type Config<'a> = ();
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(
        _config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let shading_pass = context.create_resource(&ShaderDirectory::new(Path::new(
            GBUFFER_COMBINE_SHADER,
        )) as &dyn ModuleLoader)?;
        Ok(Self { shading_pass })
    }
}

impl Destroy for GBufferCombine {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        let _ = context.destroy_resource(self.shading_pass);
        Ok(())
    }
}

unsafe impl Task for GBufferCombine {
    type Dependencies = dependency_list![GBufferWrite];
    type ResourceSet = list_type![
        ExternalResources,
        FrameCell<DeferedRenderPass<AttachmentsGBuffer>>,
        ResourceCell<PersistentCommandPool<Secondary, Graphics>>,
        DeferredSharedResources,
        TypedNil<DestroyTerminator>
    ];
    type InitializerList = Nil;
    type TaskError = ResourceError;
    type TaskResult = Frame<DeferedRenderPass<AttachmentsGBuffer>>;

    fn execute<'a>(
        &'a mut self,
        unpack_list![context, frame, command_pool, shared]: ListMutType<'a, Self::ResourceSet>,
    ) -> Result<Self::TaskResult, Self::TaskError> {
        let common_resources = &context.common_resources();
        let render_pass = context
            .get_unique_resource::<RenderPass<DeferedRenderPass<AttachmentsGBuffer>>, _>()?;
        let command = context.operate_mut(
            index_list![
                command_pool.index(),
                shared.descriptor_pool,
                self.shading_pass
            ],
            |unpack_list![shading_pass, descriptor_pool, command_pool]| {
                let command = context.begin_secondary_command::<_, _, _, GBufferShadingPass<_>>(
                    command_pool.next_command().1,
                    render_pass,
                    frame.swapchain_frame.framebuffer,
                )?;
                let command = context
                    .start_recording(command)
                    .push(&shading_pass.bind())
                    .push(&descriptor_pool.get(0).get_binding(shading_pass))
                    .push(&common_resources.draw(CommonMesh::Plane))
                    .stop_recording();
                Result::<_, ResourceError>::Ok(context.finish_command(command)?)
            },
        )??;
        frame.primary_command = Some(
            context
                .start_recording(frame.primary_command.take().unwrap())
                .push(&command)
                .push(&EndRenderPass)
                .stop_recording(),
        );
        Ok(frame.take().unwrap())
    }
}
