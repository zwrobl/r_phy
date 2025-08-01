use std::{
    any::TypeId, cell::LazyCell, collections::HashMap, convert::Infallible, error::Error,
    hash::Hash, marker::PhantomData,
};

use graphics::{
    model::{Drawable, Material, MaterialHandle, Vertex},
    shader::{ShaderHandle, ShaderType},
};

use math::types::Matrix4;
use type_kit::{unpack_list, Cons};
use vulkan_low::{
    device::raw::resources::{
        command::DrawIndexed,
        descriptor::{Descriptor, DescriptorBindingData},
        layout::{
            presets::{ModelMatrix, ModelNormalMatrix},
            DescriptorLayout,
        },
        pipeline::{
            GraphicsPipeline, GraphicsPipelinePackList, PipelineBindData, PushConstantRangeMapper,
        },
        swapchain::SwapchainFrame,
        ResourceIndexListBuilder,
    },
    index_list, Context,
};

use crate::{
    renderer::deferred::presets::{AttachmentsGBuffer, DeferedRenderPass, GBufferWritePass},
    resources::{bind_mesh_pack, MaterialPackList, MeshPackBinding, MeshPackList},
};

use super::{Commands, DeferredRendererContext, DeferredRendererFrameState, DeferredShader};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelIndex {
    mesh_index: u32,
}

impl ModelIndex {
    fn get<D: Drawable>(drawable: &D) -> Self {
        let mesh_index = drawable.mesh().index();
        Self { mesh_index }
    }
}

pub struct ModelState {
    mesh_bind_data: DrawIndexed,
    instances: Vec<Matrix4>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferIndex {
    mesh_pack_index: TypeId,
}

impl BufferIndex {
    fn get<V: Vertex>() -> Self {
        let mesh_pack_index = TypeId::of::<V>();
        Self { mesh_pack_index }
    }
}

pub struct BufferState {
    mesh_pack_binding: MeshPackBinding,
    model_states: HashMap<ModelIndex, ModelState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorIndex {
    material_pack_index: TypeId,
    material_index: u32,
}

impl DescriptorIndex {
    pub fn get<M: Material>(handle: MaterialHandle<M>) -> Self {
        let material_pack_index = TypeId::of::<M>();
        let material_index = handle.index();
        Self {
            material_pack_index,
            material_index,
        }
    }
}

pub struct DescriptorState {
    sets: Vec<DescriptorBindingData>,
    buffer_states: HashMap<BufferIndex, BufferState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipelineIndex {
    vertex_type: TypeId,
    material_type: TypeId,
    pipeline_index: usize,
}

impl PipelineIndex {
    pub fn get<S: ShaderType>(shader: ShaderHandle<S>) -> Self {
        let pipeline_index = shader.index() as usize;
        Self {
            vertex_type: TypeId::of::<S::Vertex>(),
            material_type: TypeId::of::<S::Material>(),
            pipeline_index,
        }
    }
}

pub struct PipelineState {
    pipeline_bind_data: PipelineBindData,
    push_constant_mapper: PushConstantRangeMapper,
    descriptor_states: HashMap<DescriptorIndex, DescriptorState>,
}

pub struct DrawGraph {
    // TODO: Change representation to use indexed linear buffers
    pub pipeline_states: HashMap<PipelineIndex, PipelineState>,
}

impl<P: GraphicsPipelinePackList> DeferredRendererContext<P> {
    pub(super) fn append_draw_call<
        S: ShaderType,
        D: Drawable,
        M: MaterialPackList,
        V: MeshPackList,
    >(
        &mut self,
        context: &Context,
        material_packs: &M,
        mesh_packs: &V,
        shader: ShaderHandle<S>,
        drawable: &D,
        transform: &Matrix4,
    ) {
        if let Some(mut current_frame) = self.current_frame.take() {
            let state = &mut current_frame.renderer_state;
            let pipeline_index = PipelineIndex::get(shader);
            let pipeline_state = state
                .draw_graph
                .pipeline_states
                .entry(pipeline_index)
                .or_insert_with(|| self.get_pipeline_state(shader));
            let descriptor_index = DescriptorIndex::get(drawable.material());
            let descriptor_state = pipeline_state
                .descriptor_states
                .entry(descriptor_index)
                .or_insert_with(|| {
                    let material_binding_data =
                        material_packs.try_get::<D::Material>().map(|pack| {
                            let material_descriptor = context
                                .operate_ref(
                                    index_list![pack.descriptors],
                                    |unpack_list![material_descriptor]| {
                                        let descriptor = material_descriptor
                                            .get(descriptor_index.material_index as usize);
                                        Result::<_, Infallible>::Ok(descriptor)
                                    },
                                )
                                .unwrap()
                                .unwrap();
                            self.get_descriptor_binding_data(material_descriptor, shader)
                        });
                    let camera_binding_data = Some(
                        self.get_descriptor_binding_data(current_frame.camera_descriptor, shader),
                    );
                    DescriptorState {
                        sets: [material_binding_data, camera_binding_data]
                            .into_iter()
                            .flatten()
                            .collect(),
                        buffer_states: HashMap::new(),
                    }
                });
            let mesh_pack = LazyCell::new(|| mesh_packs.try_get::<D::Vertex>().unwrap());
            let buffer_index = BufferIndex::get::<D::Vertex>();
            let buffer_state = descriptor_state
                .buffer_states
                .entry(buffer_index)
                .or_insert_with(|| BufferState {
                    mesh_pack_binding: (*mesh_pack).into(),
                    model_states: HashMap::new(),
                });
            let model_index = ModelIndex::get(drawable);
            buffer_state
                .model_states
                .entry(model_index)
                .and_modify(|model_states| model_states.instances.push(*transform))
                .or_insert_with(|| ModelState {
                    mesh_bind_data: (*mesh_pack).get(model_index.mesh_index as usize).into(),
                    instances: vec![*transform],
                });
            self.current_frame.replace(current_frame);
        }
    }

    pub(super) fn record_draw_calls(
        &mut self,
        context: &Context,
        state: DeferredRendererFrameState<P>,
        swapchain_frame: &SwapchainFrame<DeferedRenderPass<AttachmentsGBuffer>>,
    ) -> Result<Commands<P>, Box<dyn Error>> {
        let DeferredRendererFrameState {
            commands:
                Commands {
                    depth_prepass,
                    mut write_pass,
                    shading_pass,
                    skybox_pass,
                    ..
                },
            draw_graph,
            ..
        } = state;
        let renderer = self.renderer.borrow();
        let depth_prepass = context.operate_mut(
            index_list![self.pipelines.depth_prepass],
            |unpack_list![pipeline]| {
                let depth_prepass = context.record_command(depth_prepass, |command| {
                    draw_graph.pipeline_states.iter().fold(
                        command,
                        |command, (_, pipeline_state)| {
                            pipeline_state.descriptor_states.iter().fold(
                                command,
                                |command, (_, descriptor_state)| {
                                    descriptor_state.buffer_states.iter().fold(
                                        command,
                                        |command, (_, buffer_state)| {
                                            let command = bind_mesh_pack(
                                                context,
                                                command,
                                                buffer_state.mesh_pack_binding,
                                            );
                                            buffer_state.model_states.iter().fold(
                                                command,
                                                |command, (_, model_state)| {
                                                    model_state.instances.iter().fold(
                                                        command,
                                                        |command, instance| {
                                                            command
                                                        .push_constants(
                                                            pipeline
                                                                .get_push_range::<ModelMatrix>(
                                                                    &instance.into(),
                                                                ),
                                                        )
                                                        .draw_indexed(model_state.mesh_bind_data)
                                                        },
                                                    )
                                                },
                                            )
                                        },
                                    )
                                },
                            )
                        },
                    )
                });
                Result::<_, Infallible>::Ok(depth_prepass)
            },
        )??;
        for (_, pipeline_state) in draw_graph.pipeline_states {
            let command = context.operate_mut(
                index_list![self.frames.secondary_commands],
                |unpack_list![secondary_commands]| {
                    let command = context
                        .begin_secondary_command::<_, _, _, GBufferWritePass<AttachmentsGBuffer>>(
                            secondary_commands.next().1,
                            renderer.render_pass,
                            swapchain_frame.framebuffer,
                        )?;
                    let command = context.record_command(command, |command| {
                        let command = command.bind_pipeline(pipeline_state.pipeline_bind_data);
                        pipeline_state.descriptor_states.iter().fold(
                            command,
                            |command, (_, descriptor_state)| {
                                let command = descriptor_state
                                    .sets
                                    .iter()
                                    .fold(command, |c, set| c.bind_descriptor_set(set));
                                descriptor_state.buffer_states.iter().fold(
                                    command,
                                    |command, (_, buffer_state)| {
                                        let command = bind_mesh_pack(
                                            context,
                                            command,
                                            buffer_state.mesh_pack_binding,
                                        );
                                        buffer_state.model_states.iter().fold(
                                            command,
                                            |command, (_, model_state)| {
                                                model_state.instances.iter().fold(
                                                    command,
                                                    |command, instance| {
                                                        command
                                                        .push_constants(pipeline_state
                                                            .push_constant_mapper
                                                            .map_push_constant::<ModelNormalMatrix>(
                                                                &instance.into()
                                                            ).unwrap())
                                                        .draw_indexed(model_state.mesh_bind_data)
                                                    },
                                                )
                                            },
                                        )
                                    },
                                )
                            },
                        )
                    });
                    Result::<_, Box<dyn Error>>::Ok(command)
                },
            )??;
            write_pass.push(command);
        }
        Ok(Commands {
            depth_prepass,
            write_pass,
            shading_pass,
            skybox_pass,
            _phantom: PhantomData,
        })
    }

    fn get_pipeline_state<S: ShaderType>(&self, shader: ShaderHandle<S>) -> PipelineState {
        let pipeline_index = shader.index() as usize;
        let pipeline: GraphicsPipeline<DeferredShader<S>> = self
            .pipelines
            .write_pass
            .try_get()
            .unwrap()
            .get(pipeline_index);
        PipelineState {
            pipeline_bind_data: pipeline.get_binding_data(),
            push_constant_mapper: PushConstantRangeMapper::new(&pipeline),
            descriptor_states: HashMap::new(),
        }
    }

    fn get_descriptor_binding_data<S: ShaderType, L: DescriptorLayout>(
        &self,
        descriptor: Descriptor<L>,
        shader: ShaderHandle<S>,
    ) -> DescriptorBindingData {
        let pipeline_index = shader.index() as usize;
        let pipeline: GraphicsPipeline<DeferredShader<S>> = self
            .pipelines
            .write_pass
            .try_get()
            .unwrap()
            .get(pipeline_index);
        descriptor.get_binding_data(&pipeline).unwrap()
    }
}

impl DrawGraph {
    pub(super) fn new() -> Self {
        Self {
            pipeline_states: HashMap::new(),
        }
    }
}
