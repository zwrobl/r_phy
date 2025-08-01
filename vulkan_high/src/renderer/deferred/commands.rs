use std::{error::Error, marker::PhantomData};

use ash::vk;

use graphics::renderer::camera::CameraMatrices;
use type_kit::{unpack_list, Cons};
use vulkan_low::{
    device::{
        raw::resources::{
            command::{
                level::{Primary, Secondary},
                operation::Graphics,
                BeginCommand, FinishedCommand, Persistent,
            },
            descriptor::Descriptor,
            framebuffer::{ClearColor, ClearDeptStencil, ClearNone, ClearValueBuilder},
            layout::presets::CameraDescriptorSet,
            pipeline::GraphicsPipelinePackList,
            swapchain::SwapchainFrame,
            ResourceIndexListBuilder,
        },
        Device,
    },
    Context,
};

use crate::{
    renderer::deferred::presets::{
        AttachmentsGBuffer, DeferedRenderPass, GBufferDepthPrepas, GBufferShadingPass,
        GBufferSkyboxPass,
    },
    resources::{bind_mesh_pack, draw_skybox},
};

use super::DeferredRendererContext;

pub(super) struct Commands<P: GraphicsPipelinePackList> {
    pub write_pass: Vec<BeginCommand<Persistent, Secondary, Graphics>>,
    pub depth_prepass: BeginCommand<Persistent, Secondary, Graphics>,
    pub shading_pass: BeginCommand<Persistent, Secondary, Graphics>,
    pub skybox_pass: BeginCommand<Persistent, Secondary, Graphics>,
    pub _phantom: PhantomData<P>,
}

impl<P: GraphicsPipelinePackList> DeferredRendererContext<P> {
    pub(super) fn prepare_commands(
        &mut self,
        context: &Context,
        swapchain_frame: &SwapchainFrame<DeferedRenderPass<AttachmentsGBuffer>>,
        camera_descriptor: Descriptor<CameraDescriptorSet>,
        camera_matrices: &CameraMatrices,
    ) -> Result<Commands<P>, Box<dyn Error>> {
        let renderer = self.renderer.borrow();
        let index_list = ResourceIndexListBuilder::new()
            .push(renderer.frame_data.descriptors)
            .push(self.frames.secondary_commands)
            .push(self.pipelines.depth_prepass)
            .push(self.pipelines.shading_pass)
            .build();
        let (depth_prepass, shading_pass, skybox_pass) = context.opperate_mut(
            index_list,
            |unpack_list![
                shading_pass_pipeline,
                depth_prepass_pipeline,
                secondary_commands,
                descriptors,
                _rest
            ]| {
                let depth_prepass = {
                    let command = context
                        .begin_secondary_command::<_, _, _, GBufferDepthPrepas<AttachmentsGBuffer>>(
                            secondary_commands.next().1,
                            renderer.render_pass,
                            swapchain_frame.framebuffer,
                        )?;
                    context.record_command(command, |command| {
                        command
                            .bind_pipeline(&***depth_prepass_pipeline)
                            .bind_descriptor_set(
                                &camera_descriptor
                                    .get_binding_data(&depth_prepass_pipeline)
                                    .unwrap(),
                            )
                    })
                };
                let shading_pass = {
                    let command = context
                        .begin_secondary_command::<_, _, _, GBufferShadingPass<_>>(
                            secondary_commands.next().1,
                            renderer.render_pass,
                            swapchain_frame.framebuffer,
                        )?;
                    let binding_data = descriptors
                        .get(0)
                        .get_binding_data(&shading_pass_pipeline)?;
                    context.record_command(command, |command| {
                        bind_mesh_pack(
                            context,
                            command
                                .bind_pipeline(&***shading_pass_pipeline)
                                .bind_descriptor_set(&binding_data),
                            &*renderer.resources.mesh,
                        )
                        .draw_indexed(renderer.resources.mesh.get(0))
                    })
                };
                let skybox_pass = {
                    let command = context
                        .begin_secondary_command::<_, _, _, GBufferSkyboxPass<_>>(
                            secondary_commands.next().1,
                            renderer.render_pass,
                            swapchain_frame.framebuffer,
                        )?;
                    context.record_command(command, |command| {
                        draw_skybox(
                            context,
                            &renderer.resources.skybox,
                            command,
                            *camera_matrices,
                        )
                    })
                };
                Result::<_, Box<dyn Error>>::Ok((depth_prepass, shading_pass, skybox_pass))
            },
        )??;
        let write_pass = Vec::with_capacity(P::LEN);
        Ok(Commands {
            write_pass,
            depth_prepass,
            shading_pass,
            skybox_pass,
            _phantom: PhantomData,
        })
    }

    pub(super) fn record_primary_command(
        &self,
        device: &Device,
        primary_command: BeginCommand<Persistent, Primary, Graphics>,
        commands: Commands<P>,
        swapchain_frame: &SwapchainFrame<DeferedRenderPass<AttachmentsGBuffer>>,
    ) -> Result<FinishedCommand<Persistent, Primary, Graphics>, Box<dyn Error>> {
        let Commands {
            write_pass,
            depth_prepass,
            shading_pass,
            skybox_pass,
            ..
        } = commands;
        let renderer = self.renderer.borrow();
        let depth_prepass = device.finish_command(depth_prepass)?;
        let skybox_pass = device.finish_command(skybox_pass)?;
        let write_pass = write_pass
            .into_iter()
            .flat_map(|command| device.finish_command(command))
            .collect::<Vec<_>>();
        let shading_pass = device.finish_command(shading_pass)?;

        let clear_values = ClearValueBuilder::new()
            .push(ClearNone {})
            .push(ClearDeptStencil {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            })
            .push(ClearColor {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            })
            .push(ClearColor {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            })
            .push(ClearColor {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            })
            .push(ClearColor {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            });
        let primary_command = device.record_command(primary_command, |command| {
            let command = command
                .begin_render_pass(swapchain_frame, &renderer.render_pass, &clear_values)
                .write_secondary(&depth_prepass)
                .next_render_pass()
                .write_secondary(&skybox_pass)
                .next_render_pass();
            write_pass
                .into_iter()
                .fold(command, |command, write_pass| {
                    command.write_secondary(&write_pass)
                })
                .next_render_pass()
                .write_secondary(&shading_pass)
                .end_render_pass()
        });
        Ok(device.finish_command(primary_command)?)
    }
}
