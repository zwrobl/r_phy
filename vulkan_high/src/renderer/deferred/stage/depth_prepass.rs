use std::{convert::Infallible, path::Path};

use type_kit::{
    dependency_list, list_type, unpack_list, Cons, Create, Dependency, Destroy, ListMutType, Nil,
    Task, TypedNil,
};
use vulkan_low::{
    index_list,
    resources::{
        command::{Graphics, NextRenderPass, PersistentCommandPool, Secondary},
        error::ResourceError,
        layout::presets::ModelMatrix,
        pipeline::{GraphicsPipeline, ModuleLoader, ShaderDirectory},
        render_pass::RenderPass,
        storage::ResourceIndexListBuilder,
        ResourceIndex,
    },
    Context,
};

use crate::renderer::{
    deferred::{
        presets::{
            AttachmentsGBuffer, DeferedRenderPass, GBufferDepthPrepas, GBufferDepthPrepasPipeline,
        },
        stage::load_resources::LoadResources,
    },
    frame::FrameCell,
    storage::DrawStorage,
    DestroyTerminator, ExternalResources, ResourceCell,
};

const DEPTH_PREPASS_SHADER: &str = "_resources/shaders/spv/deferred/depth_prepass";

pub struct DepthPrepass {
    depth_prepass: ResourceIndex<GraphicsPipeline<GBufferDepthPrepasPipeline<AttachmentsGBuffer>>>,
}

impl Create for DepthPrepass {
    type Config<'a> = ();
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(
        _config: Self::Config<'a>,
        context: Self::Context<'b>,
    ) -> type_kit::CreateResult<Self> {
        let depth_prepass = context.create_resource(&ShaderDirectory::new(Path::new(
            DEPTH_PREPASS_SHADER,
        )) as &dyn ModuleLoader)?;
        Ok(Self { depth_prepass })
    }
}

impl Destroy for DepthPrepass {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        let _ = context.destroy_resource(self.depth_prepass);
        Ok(())
    }
}

unsafe impl Task for DepthPrepass {
    type Dependencies = dependency_list![LoadResources];
    type ResourceSet = list_type![
        ExternalResources,
        ResourceCell<PersistentCommandPool<Secondary, Graphics>>,
        FrameCell<DeferedRenderPass<AttachmentsGBuffer>>,
        DrawStorage,
        TypedNil<DestroyTerminator>
    ];
    type InitializerList = Nil;
    type TaskError = ResourceError;
    type TaskResult = ();

    #[inline]
    fn execute<'a>(
        &'a mut self,
        unpack_list![context, command_pool, frame, draw_storage]: ListMutType<
            'a,
            Self::ResourceSet,
        >,
    ) -> Result<Self::TaskResult, Self::TaskError> {
        let render_pass = context
            .get_unique_resource::<RenderPass<DeferedRenderPass<AttachmentsGBuffer>>, _>()?;
        let command = context.operate_mut(
            index_list![command_pool.index(), self.depth_prepass],
            |unpack_list![depth_prepass, command_pool]| {
                let command = context
                    .begin_secondary_command::<_, _, _, GBufferDepthPrepas<AttachmentsGBuffer>>(
                        command_pool.next_command().1,
                        render_pass,
                        frame.swapchain_frame.framebuffer,
                    )?;
                let command = context
                    .start_recording(command)
                    .push(&depth_prepass.bind())
                    .push(&frame.camera_descriptor.get_binding(depth_prepass));
                let command = draw_storage
                    .into_iter()
                    .fold(command, |command, pipeline_state| {
                        pipeline_state.process(
                            command,
                            |command, _| command,
                            |command, _| command,
                            |command, buffer_state| command.push(&buffer_state.mesh_pack_binding),
                            |command, model_state, _| {
                                model_state
                                    .instances
                                    .iter()
                                    .fold(command, |command, instance| {
                                        command
                                            .push(&depth_prepass.map::<ModelMatrix, _>(instance))
                                            .push(&model_state.mesh_bind_data)
                                    })
                            },
                        )
                    })
                    .stop_recording();
                Result::<_, ResourceError>::Ok(command)
            },
        )??;
        let command = context.finish_command(command)?;
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
