use std::{
    any::TypeId,
    collections::{hash_map::Values, HashMap},
    error::Error,
    hash::Hash,
    marker::PhantomData,
};

use graphics::{
    model::{Drawable, Material, MaterialHandle, MeshHandle, Vertex},
    shader::{ShaderHandle, ShaderType},
};

use math::types::Matrix4;
use type_kit::{unpack_list, Cons};
use vulkan_low::{
    index_list,
    resources::{
        command::DrawIndexed,
        descriptor::{Descriptor, DescriptorBindingData},
        layout::presets::{CameraDescriptorSet, ModelMatrix, ModelNormalMatrix},
        pipeline::{PipelineBindData, PushConstantRangeMapper},
        storage::ResourceIndexListBuilder,
        swapchain::SwapchainFrame,
    },
    Context,
};

use crate::{
    renderer::deferred::{
        presets::{AttachmentsGBuffer, DeferedRenderPass, GBufferWritePass},
        DeferredRendererContext, DeferredRendererFrameState, DeferredShader,
    },
    resources::{
        bind_mesh_pack, GraphicsPipelinePackList, MaterialPackList, MeshPackBinding, MeshPackList,
    },
};

use super::CommandStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshIndex {
    mesh_index: u32,
}

impl MeshIndex {
    fn get<V: Vertex>(mesh: MeshHandle<V>) -> Self {
        let mesh_index = mesh.index();
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
    model_states: HashMap<MeshIndex, ModelState>,
}

impl BufferState {
    fn push_model_state<V: Vertex, L: MeshPackList>(
        &mut self,
        model_index: MeshHandle<V>,
        transform: &Matrix4,
        mesh_packs: &L,
    ) -> &mut ModelState {
        let mesh_index = MeshIndex::get(model_index);
        self.model_states
            .entry(mesh_index)
            .and_modify(|model_states| model_states.instances.push(*transform))
            .or_insert_with(|| ModelState {
                mesh_bind_data: (mesh_packs.get::<V>())
                    .get(mesh_index.mesh_index as usize)
                    .into(),
                instances: vec![*transform],
            })
    }
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

impl DescriptorState {
    fn get_buffer_state<V: Vertex, L: MeshPackList>(&mut self, mesh_packs: &L) -> &mut BufferState {
        let buffer_index = BufferIndex::get::<V>();
        self.buffer_states
            .entry(buffer_index)
            .or_insert_with(|| BufferState {
                mesh_pack_binding: mesh_packs.get::<V>().into(),
                model_states: HashMap::new(),
            })
    }
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

impl PipelineState {
    fn get_descriptor_state<
        M: Material,
        S: ShaderType<Material = M>,
        L: MaterialPackList,
        P: GraphicsPipelinePackList,
    >(
        &mut self,
        context: &Context,
        camera: Descriptor<CameraDescriptorSet>,
        shader: ShaderHandle<S>,
        material: MaterialHandle<M>,
        material_packs: &L,
        pipelines: &P,
    ) -> &mut DescriptorState {
        let descriptor_index = DescriptorIndex::get(material);
        if !self.descriptor_states.contains_key(&descriptor_index) {
            self.insert_descriptor_state(
                context,
                camera,
                shader,
                material,
                material_packs,
                pipelines,
            );
        }
        self.descriptor_states.get_mut(&descriptor_index).unwrap()
    }

    fn insert_descriptor_state<
        M: Material,
        S: ShaderType<Material = M>,
        L: MaterialPackList,
        P: GraphicsPipelinePackList,
    >(
        &mut self,
        context: &Context,
        camera: Descriptor<CameraDescriptorSet>,
        shader: ShaderHandle<S>,
        material: MaterialHandle<M>,
        material_packs: &L,
        pipelines: &P,
    ) {
        let pipeline_index = pipelines
            .get::<DeferredShader<S>>()
            .get(shader.index() as usize);
        let descriptor_index = DescriptorIndex::get(material);
        let material_binding_data = material_packs.try_get::<M>().map(|pack| {
            pack.get_descriptor_binding_data(
                context,
                descriptor_index.material_index,
                pipeline_index,
            )
            .unwrap()
        });
        let camera_binding_data = context
            .operate_ref(index_list![pipeline_index], |unpack_list![pipeline]| {
                camera.get_binding_data(pipeline)
            })
            .unwrap();
        let state = DescriptorState {
            sets: [material_binding_data, Some(camera_binding_data)]
                .into_iter()
                .flatten()
                .collect(),
            buffer_states: HashMap::new(),
        };
        self.descriptor_states.insert(descriptor_index, state);
    }

    fn process<
        T,
        F1: Fn(T, &PipelineState) -> T,
        F2: Fn(T, &DescriptorState) -> T,
        F3: Fn(T, &BufferState) -> T,
        F4: Fn(T, &ModelState, &PushConstantRangeMapper) -> T,
    >(
        &self,
        init: T,
        pipeline_state_fn: F1,
        descriptor_state_fn: F2,
        buffer_state_fn: F3,
        model_state_fn: F4,
    ) -> T {
        self.descriptor_states.values().fold(
            pipeline_state_fn(init, self),
            |acc, descriptor_state| {
                let acc = descriptor_state_fn(acc, descriptor_state);
                descriptor_state
                    .buffer_states
                    .values()
                    .fold(acc, |acc, buffer_state| {
                        let acc = buffer_state_fn(acc, buffer_state);
                        buffer_state
                            .model_states
                            .values()
                            .fold(acc, |acc, model_state| {
                                model_state_fn(acc, model_state, &self.push_constant_mapper)
                            })
                    })
            },
        )
    }
}

pub struct DrawStorage {
    // TODO: Change representation to use indexed linear buffers
    pub pipeline_states: HashMap<PipelineIndex, PipelineState>,
}

impl Default for DrawStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawStorage {
    pub fn new() -> Self {
        Self {
            pipeline_states: HashMap::new(),
        }
    }

    fn get_pipeline_state<'a, S: ShaderType, P: GraphicsPipelinePackList>(
        &'a mut self,
        context: &Context,
        shader: ShaderHandle<S>,
        pipelines: &P,
    ) -> &'a mut PipelineState {
        let pipeline_index = PipelineIndex::get(shader);
        if !self.pipeline_states.contains_key(&pipeline_index) {
            self.insert_pipeline_state(context, shader, pipelines);
        };
        self.pipeline_states.get_mut(&pipeline_index).unwrap()
    }

    fn insert_pipeline_state<S: ShaderType, P: GraphicsPipelinePackList>(
        &mut self,
        context: &Context,
        shader: ShaderHandle<S>,
        pipelines: &P,
    ) {
        let pipeline = pipelines
            .get::<DeferredShader<S>>()
            .get(shader.index() as usize);
        let (pipeline_bind_data, push_constant_mapper) = context
            .operate_ref(index_list![pipeline], |unpack_list![pipeline]| {
                let binding = pipeline.get_binding_data();
                let mapper = PushConstantRangeMapper::new(pipeline);
                (binding, mapper)
            })
            .unwrap();
        let pipeline_state = PipelineState {
            pipeline_bind_data,
            push_constant_mapper,
            descriptor_states: HashMap::new(),
        };
        self.pipeline_states
            .insert(PipelineIndex::get(shader), pipeline_state);
    }
}

impl<'a> IntoIterator for &'a DrawStorage {
    type Item = &'a PipelineState;
    type IntoIter = Values<'a, PipelineIndex, PipelineState>;

    fn into_iter(self) -> Self::IntoIter {
        self.pipeline_states.values()
    }
}

impl<'a, P: GraphicsPipelinePackList> DeferredRendererContext<'a, P> {
    pub fn append_draw_call<
        D: Drawable,
        S: ShaderType<Material = D::Material, Vertex = D::Vertex>,
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
            let pipeline_state =
                state
                    .draw_graph
                    .get_pipeline_state(context, shader, &self.pipelines.write_pass);
            let descriptor_state = pipeline_state.get_descriptor_state(
                context,
                current_frame.camera_descriptor,
                shader,
                drawable.material(),
                material_packs,
                &self.pipelines.write_pass,
            );
            let buffer_state = descriptor_state.get_buffer_state::<D::Vertex, _>(mesh_packs);
            buffer_state.push_model_state(drawable.mesh(), transform, mesh_packs);
            self.current_frame.replace(current_frame);
        }
    }

    pub fn record_draw_calls(
        &mut self,
        context: &Context,
        state: DeferredRendererFrameState<P>,
        swapchain_frame: &SwapchainFrame<DeferedRenderPass<AttachmentsGBuffer>>,
    ) -> Result<CommandStorage<P>, Box<dyn Error>> {
        let DeferredRendererFrameState {
            commands:
                CommandStorage {
                    depth_prepass,
                    shading_pass,
                    skybox_pass,
                    mut write_pass,
                    ..
                },
            draw_graph,
            ..
        } = state;
        let depth_prepass = context
            .operate_ref(
                index_list![self.pipelines.depth_prepass],
                |unpack_list![pipeline]| {
                    draw_graph
                        .into_iter()
                        .fold(
                            context.start_recording(depth_prepass),
                            |command, pipeline_state| {
                                pipeline_state.process(
                                    command,
                                    |command, _| command,
                                    |command, _| command,
                                    |command, buffer_state| {
                                        bind_mesh_pack(
                                            context,
                                            command,
                                            buffer_state.mesh_pack_binding,
                                        )
                                    },
                                    |command, model_state, _| {
                                        model_state.instances.iter().fold(
                                            command,
                                            |command, instance| {
                                                command
                                                    .push_constants(
                                                        pipeline.get_push_range::<ModelMatrix>(
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
                        .stop_recording()
                },
            )
            .unwrap();
        context.operate_mut(
            index_list![self.frames.secondary_commands],
            |unpack_list![secondary_commands]| {
                draw_graph.into_iter().for_each(|pipeline_state| {
                    let command = context
                        .begin_secondary_command::<_, _, _, GBufferWritePass<AttachmentsGBuffer>>(
                            secondary_commands.next_command().1,
                            self.renderer.render_pass,
                            swapchain_frame.framebuffer,
                        )
                        .unwrap();
                    let command = pipeline_state
                        .process(
                            context.start_recording(command),
                            |command, pipeline_state| {
                                command.bind_pipeline(pipeline_state.pipeline_bind_data)
                            },
                            |command, descriptor_state| {
                                descriptor_state
                                    .sets
                                    .iter()
                                    .fold(command, |c, set| c.bind_descriptor_set(set))
                            },
                            |command, buffer_state| {
                                bind_mesh_pack(context, command, buffer_state.mesh_pack_binding)
                            },
                            |command, model_state, push_constant_mapper| {
                                model_state
                                    .instances
                                    .iter()
                                    .fold(command, |command, instance| {
                                        command
                                            .push_constants(
                                                push_constant_mapper
                                                    .map_push_constant::<ModelNormalMatrix>(
                                                        &instance.into(),
                                                    )
                                                    .unwrap(),
                                            )
                                            .draw_indexed(model_state.mesh_bind_data)
                                    })
                            },
                        )
                        .stop_recording();
                    write_pass.push(command);
                });
            },
        )?;
        Ok(CommandStorage {
            depth_prepass,
            write_pass,
            shading_pass,
            skybox_pass,
            _phantom: PhantomData,
        })
    }
}
