use std::convert::Infallible;

use type_kit::{
    Cons, Dependency, Destroy, MutList, Nil, Task, TypedNil, dependency_list, list_type,
    unpack_list,
};
use vulkan_low::{
    Context, index_list,
    resources::{
        command::{Graphics, NextRenderPass, PersistentCommandPool, Secondary},
        error::ResourceError,
        layout::presets::ModelNormalMatrix,
        render_pass::RenderPass,
        storage::ResourceIndexListBuilder,
    },
};

use crate::renderer::{
    DestroyTerminator, ExternalResources, ResourceCell,
    deferred::{
        presets::{AttachmentsGBuffer, DeferedRenderPass, GBufferWritePass},
        stage::draw_skybox::DrawSkybox,
    },
    frame::FrameCell,
    storage::DrawStorage,
};

pub struct GBufferWrite;

impl Destroy for GBufferWrite {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        Ok(())
    }
}

unsafe impl Task for GBufferWrite {
    type Dependencies = dependency_list![DrawSkybox];
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

    fn execute<'a>(
        &'a mut self,
        unpack_list![context, command_pool, frame, draw_storage]: MutList<'a, Self::ResourceSet>,
    ) -> Result<Self::TaskResult, Self::TaskError> {
        let context = &context.context;
        let render_pass = context
            .get_unique_resource::<RenderPass<DeferedRenderPass<AttachmentsGBuffer>>, _>()?;
        let commands = context.operate_mut(
            index_list![command_pool.index()],
            |unpack_list![command_pool]| {
                let commands = draw_storage
                    .into_iter()
                    .map(|pipeline_state| {
                        context
                            .begin_secondary_command::<_, _, _, GBufferWritePass<_>>(
                                command_pool.next_command().1,
                                render_pass,
                                frame.swapchain_frame.framebuffer,
                            )
                            .and_then(|command| {
                                let command = pipeline_state
                                    .process(
                                        context.start_recording(command),
                                        |command, pipeline_state| {
                                            command.push(&pipeline_state.bind_data)
                                        },
                                        |command, descriptor_state| {
                                            command.extend(&descriptor_state.sets)
                                        },
                                        |command, buffer_state| {
                                            command.push(&buffer_state.mesh_pack_binding)
                                        },
                                        |command, model_state, push_constant_mapper| {
                                            model_state.instances.iter().fold(
                                                command,
                                                |command, instance| {
                                                    command
                                                        .push(
                                                            &push_constant_mapper
                                                                .map::<ModelNormalMatrix>(instance),
                                                        )
                                                        .push(&model_state.mesh_bind_data)
                                                },
                                            )
                                        },
                                    )
                                    .stop_recording();
                                context.finish_command(command)
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Result::<_, ResourceError>::Ok(commands)
            },
        )??;
        frame.primary_command = Some(
            context
                .start_recording(frame.primary_command.take().unwrap())
                .extend(&commands)
                .push(&NextRenderPass)
                .stop_recording(),
        );
        Ok(())
    }
}
