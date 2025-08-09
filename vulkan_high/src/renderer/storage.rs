use std::{
    any::TypeId,
    collections::{hash_map::Values, HashMap},
    convert::Infallible,
    hash::Hash,
};

use graphics::{
    model::{Drawable, Material, MaterialHandle, MeshHandle, Vertex},
    shader::{ShaderHandle, ShaderType},
};

use math::types::Matrix4;
use type_kit::{unpack_list, Cons, Destroy};
use vulkan_low::{
    index_list,
    resources::{
        command::{BindPipeline, DrawIndexed},
        descriptor::{Descriptor, DescriptorBindingData},
        layout::presets::CameraDescriptorSet,
        pipeline::{GraphicsPipelineConfig, PushConstantRangeMapper},
        storage::ResourceIndexListBuilder,
    },
    Context,
};

use crate::{
    renderer::{Renderer, ShaderDescriptor},
    resources::{
        GraphicsPipelinePackList, MaterialPackList, MeshPackList, PackBufferBindings, ResourcePack,
    },
};

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
    pub mesh_bind_data: DrawIndexed,
    pub instances: Vec<Matrix4>,
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
    pub mesh_pack_binding: PackBufferBindings,
    pub model_states: HashMap<MeshIndex, ModelState>,
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
pub struct DescriptorSetIndex {
    material_pack_index: TypeId,
    material_index: u32,
}

impl DescriptorSetIndex {
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
    pub sets: Vec<DescriptorBindingData>,
    buffer_states: HashMap<BufferIndex, BufferState>,
}

impl DescriptorState {
    fn get_buffer_state<V: Vertex, L: MeshPackList>(&mut self, mesh_packs: &L) -> &mut BufferState {
        let buffer_index = BufferIndex::get::<V>();
        self.buffer_states
            .entry(buffer_index)
            .or_insert_with(|| BufferState {
                mesh_pack_binding: mesh_packs.get::<V>().bindings(),
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
    pub bind_data: BindPipeline,
    pub push_constant_mapper: PushConstantRangeMapper,
    pub descriptor_states: HashMap<DescriptorSetIndex, DescriptorState>,
}

impl PipelineState {
    fn get_descriptor_state<
        M: Material,
        S: ShaderType<Material = M> + ShaderDescriptor<CameraDescriptorSet>,
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
        let descriptor_index = DescriptorSetIndex::get(material);
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
        S: ShaderType<Material = M> + ShaderDescriptor<CameraDescriptorSet>,
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
        let pipeline_index = pipelines.get::<S>().get(shader.index() as usize);
        let descriptor_index = DescriptorSetIndex::get(material);
        let material_binding_data = material_packs.try_get::<M>().map(|pack| {
            pack.try_get_descriptor_binding_data(
                context,
                descriptor_index.material_index,
                pipeline_index,
            )
            .unwrap()
        });
        let camera_binding_data = S::get_mapper().get_binding(context, camera).unwrap();
        let state = DescriptorState {
            sets: [material_binding_data, Some(camera_binding_data)]
                .into_iter()
                .flatten()
                .collect(),
            buffer_states: HashMap::new(),
        };
        self.descriptor_states.insert(descriptor_index, state);
    }

    pub fn process<
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

    fn get_pipeline_state<
        'a,
        S: ShaderType + GraphicsPipelineConfig,
        P: GraphicsPipelinePackList,
    >(
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

    fn insert_pipeline_state<
        S: ShaderType + GraphicsPipelineConfig,
        P: GraphicsPipelinePackList,
    >(
        &mut self,
        context: &Context,
        shader: ShaderHandle<S>,
        pipelines: &P,
    ) {
        let pipeline = pipelines.get::<S>().get(shader.index() as usize);
        let (bind_data, push_constant_mapper) = context
            .operate_ref(index_list![pipeline], |unpack_list![pipeline]| {
                let binding = pipeline.bind();
                let mapper = PushConstantRangeMapper::new(pipeline);
                (binding, mapper)
            })
            .unwrap();
        let pipeline_state = PipelineState {
            bind_data,
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

impl Destroy for DrawStorage {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, _context: &Context) -> Result<(), Self::DestroyError> {
        self.pipeline_states.clear();
        Ok(())
    }
}

pub struct DrawStorageTyped<
    R: Renderer,
    M: MaterialPackList,
    V: MeshPackList,
    P: GraphicsPipelinePackList,
> {
    camera: Option<Descriptor<CameraDescriptorSet>>,
    storage: Option<DrawStorage>,
    resources: ResourcePack<R, M, V, P>,
}

impl<R: Renderer, M: MaterialPackList, V: MeshPackList, P: GraphicsPipelinePackList>
    DrawStorageTyped<R, M, V, P>
{
    #[inline]
    pub fn new(resources: ResourcePack<R, M, V, P>) -> Self {
        Self {
            camera: None,
            storage: None,
            resources,
        }
    }

    #[inline]
    pub fn begin_frame(&mut self, camera: Descriptor<CameraDescriptorSet>) {
        self.camera = Some(camera);
        self.storage = Some(DrawStorage::new());
    }

    #[inline]
    pub fn append_draw_call<
        D: Drawable,
        S: ShaderType<Material = D::Material, Vertex = D::Vertex>
            + ShaderDescriptor<CameraDescriptorSet>,
    >(
        &mut self,
        context: &Context,
        shader: ShaderHandle<S>,
        drawable: &D,
        transform: &Matrix4,
    ) {
        let camera = self.camera.unwrap();
        let pipeline_state = self.storage.as_mut().unwrap().get_pipeline_state(
            context,
            shader,
            &self.resources.pipelines,
        );
        let descriptor_state = pipeline_state.get_descriptor_state(
            context,
            camera,
            shader,
            drawable.material(),
            &self.resources.materials,
            &self.resources.pipelines,
        );
        let buffer_state =
            descriptor_state.get_buffer_state::<D::Vertex, _>(&self.resources.meshes);
        buffer_state.push_model_state(drawable.mesh(), transform, &self.resources.meshes);
    }

    #[inline]
    pub fn end_frame(&mut self) -> DrawStorage {
        let storage = self.storage.take().unwrap();
        self.camera = None;
        storage
    }
}

impl<R: Renderer, M: MaterialPackList, V: MeshPackList, P: GraphicsPipelinePackList> Destroy
    for DrawStorageTyped<R, M, V, P>
{
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy(&mut self, context: &Context) -> Result<(), Self::DestroyError> {
        self.resources.destroy(context)?;
        self.storage = None;
        self.camera = None;
        Ok(())
    }
}
