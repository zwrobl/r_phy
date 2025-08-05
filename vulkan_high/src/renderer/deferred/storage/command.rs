use std::{error::Error, marker::PhantomData};

use graphics::renderer::camera::CameraMatrices;
use type_kit::{unpack_list, Cons};
use vulkan_low::{
    error::ExtError,
    index_list,
    resources::{
        command::{
            BeginCommand, EndRenderPass, FinishedCommand, Graphics, NextRenderPass, Persistent,
            Primary, Secondary,
        },
        descriptor::Descriptor,
        framebuffer::{ClearColor, ClearDeptStencil, ClearNone, ClearValueBuilder},
        layout::presets::CameraDescriptorSet,
        storage::ResourceIndexListBuilder,
        swapchain::SwapchainFrame,
    },
    Context,
};

use crate::{
    renderer::deferred::{
        presets::{
            AttachmentsGBuffer, DeferedRenderPass, GBufferDepthPrepas, GBufferShadingPass,
            GBufferSkyboxPass,
        },
        DeferredRendererContext,
    },
    resources::{CommonMesh, CommonResources, GraphicsPipelinePackList},
};

pub struct CommandStorage<P: GraphicsPipelinePackList> {
    pub write_pass: Vec<BeginCommand<Persistent, Secondary, Graphics>>,
    pub depth_prepass: BeginCommand<Persistent, Secondary, Graphics>,
    pub shading_pass: BeginCommand<Persistent, Secondary, Graphics>,
    pub skybox_pass: BeginCommand<Persistent, Secondary, Graphics>,
    pub _phantom: PhantomData<P>,
}

impl<'a, P: GraphicsPipelinePackList> DeferredRendererContext<'a, P> {
    pub fn prepare_commands(
        &mut self,
        context: &Context,
        common_meshes: &CommonResources,
        swapchain_frame: &SwapchainFrame<DeferedRenderPass<AttachmentsGBuffer>>,
        camera_descriptor: Descriptor<CameraDescriptorSet>,
        camera_matrices: &CameraMatrices,
    ) -> Result<CommandStorage<P>, Box<dyn Error>> {
        let renderer = self.renderer;
        let (depth_prepass, shading_pass, skybox_pass) = context.operate_mut(
            index_list![
                renderer.frame_data.descriptors,
                self.frames.secondary_commands,
                self.pipelines.depth_prepass,
                self.pipelines.shading_pass
            ],
            |unpack_list![
                shading_pass_pipeline,
                depth_prepass_pipeline,
                secondary_commands,
                descriptors
            ]| {
                let depth_prepass = {
                    let command = context
                        .begin_secondary_command::<_, _, _, GBufferDepthPrepas<AttachmentsGBuffer>>(
                            secondary_commands.next_command().1,
                            renderer.render_pass,
                            swapchain_frame.framebuffer,
                        )?;
                    context
                        .start_recording(command)
                        .push(&depth_prepass_pipeline.bind())
                        .push(&camera_descriptor.bind(depth_prepass_pipeline))
                        .stop_recording()
                };
                let shading_pass = {
                    let command = context
                        .begin_secondary_command::<_, _, _, GBufferShadingPass<_>>(
                            secondary_commands.next_command().1,
                            renderer.render_pass,
                            swapchain_frame.framebuffer,
                        )?;
                    context
                        .start_recording(command)
                        .push(&shading_pass_pipeline.bind())
                        .push(&descriptors.get(0).bind(shading_pass_pipeline))
                        .push(&common_meshes.draw(CommonMesh::Plane))
                        .stop_recording()
                };
                let skybox_pass = {
                    let command = context
                        .begin_secondary_command::<_, _, _, GBufferSkyboxPass<_>>(
                            secondary_commands.next_command().1,
                            renderer.render_pass,
                            swapchain_frame.framebuffer,
                        )?;
                    context
                        .start_recording(command)
                        .push(
                            &renderer
                                .resources
                                .skybox
                                .draw(common_meshes, *camera_matrices),
                        )
                        .stop_recording()
                };
                Result::<_, ExtError>::Ok((depth_prepass, shading_pass, skybox_pass))
            },
        )??;
        let write_pass = Vec::with_capacity(P::LEN);
        Ok(CommandStorage {
            write_pass,
            depth_prepass,
            shading_pass,
            skybox_pass,
            _phantom: PhantomData,
        })
    }

    pub fn record_primary_command(
        &self,
        context: &Context,
        primary_command: BeginCommand<Persistent, Primary, Graphics>,
        commands: CommandStorage<P>,
        swapchain_frame: &SwapchainFrame<DeferedRenderPass<AttachmentsGBuffer>>,
    ) -> Result<FinishedCommand<Persistent, Primary, Graphics>, Box<dyn Error>> {
        let CommandStorage {
            write_pass,
            depth_prepass,
            shading_pass,
            skybox_pass,
            ..
        } = commands;
        let renderer = self.renderer;
        let depth_prepass = context.finish_command(depth_prepass)?;
        let skybox_pass = context.finish_command(skybox_pass)?;
        let write_pass = write_pass
            .into_iter()
            .flat_map(|command| context.finish_command(command))
            .collect::<Vec<_>>();
        let shading_pass = context.finish_command(shading_pass)?;

        let clear_values = ClearValueBuilder::new()
            .push(ClearNone)
            .push(ClearDeptStencil::new(1.0, 0))
            .push(ClearColor::new([0.0, 0.0, 0.0, 1.0]))
            .push(ClearColor::new([0.0, 0.0, 0.0, 1.0]))
            .push(ClearColor::new([0.0, 0.0, 0.0, 1.0]))
            .push(ClearColor::new([0.0, 0.0, 0.0, 1.0]));
        let primary_command = context
            .start_recording(primary_command)
            .push(&renderer.render_pass.begin(swapchain_frame, &clear_values))
            .push(&depth_prepass)
            .push(&NextRenderPass)
            .push(&skybox_pass)
            .push(&NextRenderPass)
            .extend(&write_pass)
            .push(&NextRenderPass)
            .push(&shading_pass)
            .push(&EndRenderPass)
            .stop_recording();
        Ok(context.finish_command(primary_command)?)
    }
}
