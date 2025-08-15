use entity::{
    component_list_type,
    context::{EntityComponentContext, EntityComponentStorage},
    ecs_context_type, entity_type,
    index::EntityIndex,
    marker_type,
    operation::OperationSender,
    stage::Builder,
};
use graphics::{
    model::{
        CommonVertex, EmptyMaterial, MeshHandleTyped, Model, ModelTyped, PbrMaterial, SimpleVertex,
        UnlitMaterial,
    },
    renderer::{camera::first_person::FirstPersonCamera, ContextBuilder},
    shader::{Shader, ShaderHandle},
};
use std::{error::Error, path::Path, result::Result};
use type_kit::{list_type, unpack_list, Cons, GenVec, GenVecIndex, Here, Nil, There, TypeList};
use vulkan_high::{
    renderer::deferred::{DeferredRendererBuilder, DeferredRendererConfig},
    VulkanRendererBuilder, VulkanRendererConfig,
};
use winit::{
    dpi::PhysicalSize,
    window::{WindowBuilder, WindowButtons},
};

use entity::system::System;
use math::{
    transform::Transform,
    types::{Matrix4, Vector2, Vector3},
};
use physics::shape::Cube;
use system::{
    system::{frame::FrameData, renderer::RenderingSystem},
    LoopBuilder,
};

#[derive(Debug, Clone, Copy)]
pub struct SpinningData {
    axis: Vector3,
    speed: f32,
}

impl SpinningData {
    fn new(axis: Vector3, speed: f32) -> Self {
        Self { axis, speed }
    }
}

type EntityComponent = ecs_context_type![Model, ShaderHandle, Transform, SpinningData, Nil];

struct SpinningSystem;

impl System<EntityComponent> for SpinningSystem {
    type External = list_type![FrameData, Nil];
    type WriteList = list_type![Transform, Nil];
    type Components = list_type![Transform, SpinningData, Nil];

    fn execute<'a>(
        &self,
        entity: EntityIndex,
        unpack_list![transform, spinning_data]: <Self::Components as TypeList>::RefList<'a>,
        context: &EntityComponent,
        queue: &OperationSender<EntityComponent>,
        unpack_list![frame_data]: <Self::External as TypeList>::RefList<'a>,
    ) {
        let transform = Transform::identity().rotate(
            spinning_data.axis,
            frame_data.delta_time() * spinning_data.speed,
        ) * *transform;
        let update = context
            .get_entity_update_builder(self, entity.in_context())
            .update(transform);
        queue.update_entity(update);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let resolution = Vector2::new(800.0, 600.0);
    let proj = Matrix4::perspective(std::f32::consts::FRAC_PI_3, 600.0 / 800.0, 1e-3, 1e3);
    let window_builder = WindowBuilder::new()
        .with_inner_size(PhysicalSize {
            width: resolution.x as u32,
            height: resolution.y as u32,
        })
        .with_resizable(false)
        .with_enabled_buttons(WindowButtons::CLOSE | WindowButtons::MINIMIZE)
        .with_title("r_phy")
        .with_transparent(false);
    let game_loop = LoopBuilder::new(
        VulkanRendererBuilder::<DeferredRendererBuilder>::new(DeferredRendererConfig::new(
            Path::new("_resources/assets/skybox/skybox"),
        ))
        .with_config(VulkanRendererConfig::builder().build()?),
    )
    .with_window(window_builder)
    .with_camera(FirstPersonCamera::new(proj, resolution))
    .build()?;
    let mut renderer_context = game_loop
        .renderer_context_builder()
        .with_material_type::<UnlitMaterial>()
        .with_material_type::<PbrMaterial>()
        .with_material_type::<EmptyMaterial>()
        .with_mesh_type::<CommonVertex>()
        .with_mesh_type::<SimpleVertex>()
        .with_shader_type::<CommonVertex, EmptyMaterial>()
        .with_shader_type::<CommonVertex, UnlitMaterial>()
        .with_shader_type::<CommonVertex, PbrMaterial>();
    let empty_material = renderer_context.add_material(EmptyMaterial::default());
    let cube_mesh: MeshHandleTyped<CommonVertex> =
        renderer_context.add_mesh(Cube::new(1.0f32).into());
    let checker_shader = renderer_context.add_shader(Shader::<CommonVertex, EmptyMaterial>::new(
        "_resources/shaders/spv/deferred/gbuffer_write/checker",
    ));
    let model = ModelTyped::new(cube_mesh, empty_material);
    let systems_context = game_loop
        .system_builder()
        .with_system(RenderingSystem)
        .with_system(SpinningSystem);
    let mut scene = game_loop.scene(renderer_context, systems_context);
    let entity = scene
        .get_entity_builder()
        .with_component::<ShaderHandle, _>(checker_shader.into())
        .with_component::<Model, _>(model.into())
        .with_component::<Transform, _>(
            Transform::identity().translate(Vector3::new(4.0, 0.0, 0.0)),
        )
        .with_component::<SpinningData, _>(SpinningData::new(
            Vector3::z(),
            std::f32::consts::FRAC_PI_2,
        ));
    scene.with_entity(entity);
    let entity = scene
        .get_entity_builder()
        .with_component::<ShaderHandle, _>(checker_shader.into())
        .with_component::<Model, _>(model.into())
        .with_component::<Transform, _>(
            Transform::identity().translate(Vector3::new(4.0, 2.0, 0.0)),
        )
        .with_component::<SpinningData, _>(SpinningData::new(
            Vector3::y(),
            std::f32::consts::FRAC_PI_2,
        ));
    scene.with_entity(entity);
    game_loop.run(scene)?;
    Ok(())
}
