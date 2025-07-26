use std::{error::Error, marker::PhantomData};

use ash::vk;

use crate::context::device::{
    framebuffer::{
        presets::AttachmentsGBuffer, ClearColor, ClearDeptStencil, ClearNone, ClearValueBuilder,
    },
    raw::resources::command::{
        level::{Primary, Secondary},
        operation::Graphics,
        BeginCommand, FinishedCommand, Persistent,
    },
    raw::resources::descriptor::Descriptor,
    raw::resources::pipeline::GraphicsPipelinePackList,
    raw::resources::{
        layout::presets::CameraDescriptorSet,
        render_pass::presets::{GBufferDepthPrepas, GBufferShadingPass, GBufferSkyboxPass},
    },
    swapchain::SwapchainFrame,
    Device,
};
use graphics::renderer::camera::CameraMatrices;

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
        device: &Device,
        swapchain_frame: &SwapchainFrame<AttachmentsGBuffer>,
        camera_descriptor: Descriptor<CameraDescriptorSet>,
        camera_matrices: &CameraMatrices,
    ) -> Result<Commands<P>, Box<dyn Error>> {
        let renderer = self.renderer.borrow();
        let depth_prepass = {
            let (_, command) = self.frames.secondary_commands.next();
            device.record_command(
                device.begin_secondary_command::<_, _, _, GBufferDepthPrepas<AttachmentsGBuffer>>(
                    command,
                    renderer.render_pass,
                    swapchain_frame.framebuffer,
                )?,
                |command| {
                    command
                        .bind_pipeline(&*self.pipelines.depth_prepass)
                        .bind_descriptor_set(
                            &camera_descriptor
                                .get_binding_data(&self.pipelines.depth_prepass)
                                .unwrap(),
                        )
                },
            )
        };
        let (_, shading_pass) = self.frames.secondary_commands.next();
        let shading_pass = device.begin_secondary_command::<_, _, _, GBufferShadingPass<_>>(
            shading_pass,
            renderer.render_pass,
            swapchain_frame.framebuffer,
        )?;
        let shading_pass = device.record_command(shading_pass, |command| {
            command
                .bind_pipeline(&*self.pipelines.shading_pass)
                .bind_descriptor_set(
                    &renderer
                        .frame_data
                        .descriptors
                        .get(0)
                        .get_binding_data(&self.pipelines.shading_pass)
                        .unwrap(),
                )
                .bind_mesh_pack(&*renderer.resources.mesh)
                .draw_mesh(renderer.resources.mesh.get(0))
        });
        let (_, skybox_pass) = self.frames.secondary_commands.next();
        let skybox_pass = device.begin_secondary_command::<_, _, _, GBufferSkyboxPass<_>>(
            skybox_pass,
            renderer.render_pass,
            swapchain_frame.framebuffer,
        )?;
        let skybox_pass = device.record_command(skybox_pass, |command| {
            command.draw_skybox(&renderer.resources.skybox, *camera_matrices)
        });
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
        swapchain_frame: &SwapchainFrame<AttachmentsGBuffer>,
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
