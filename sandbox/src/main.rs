use entity::{
    component_list_type,
    context::EntityComponentStorage,
    ecs_context_type,
    entity::ComponentUpdate,
    entity_type,
    index::EntityIndex,
    marker_type,
    operation::{AddComponent, OperationChannel},
    stage::{Builder, Parallel, Synchronous},
};
use graphics::{
    model::{
        CommonVertex, EmptyMaterial, MeshHandleTyped, Model, ModelTyped, PbrMaterial, SimpleVertex,
        UnlitMaterial,
    },
    renderer::{camera::ProjectionMatrix, ContextBuilder},
    shader::{Shader, ShaderHandle},
};
use std::{error::Error, path::Path, result::Result};
use type_kit::{list_type, unpack_list, Cons, GenVec, GenVecIndex, Here, Nil, RefList, There};
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
    types::{Vector2, Vector3},
};
use physics::shape::Cube;
use system::{
    system::{
        command::{Command, CommandQueue},
        control::{FirstPerson, FirstPersonController, KeyBindings},
        frame::FrameData,
        input::{GlobalInput, InputSystem, Key, KeyState},
        renderer::{CameraSelector, DrawCommandSystem},
    },
    LoopBuilder,
};

#[derive(Debug, Clone, Copy)]
pub struct SpawnerData {
    shader: ShaderHandle,
    mesh: Model,
}

impl SpawnerData {
    pub fn new(shader: ShaderHandle, mesh: Model) -> Self {
        Self { shader, mesh }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpinningData {
    axis: Vector3,
    speed: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DestroyCountdown {
    pub remaining: f32,
}

pub struct Destroy;

impl System<EntityComponent> for Destroy {
    type External = list_type![FrameData, Nil];
    type WriteList = list_type![DestroyCountdown, Nil];
    type Components = list_type![DestroyCountdown, Nil];

    fn execute<'a>(
        &self,
        entity: EntityIndex,
        unpack_list![countdown]: RefList<'a, Self::Components>,
        _context: &EntityComponent,
        queue: &OperationChannel<'_, EntityComponent>,
        unpack_list![frame_data]: RefList<'a, Self::External>,
    ) {
        if countdown.remaining <= 0.0 {
            queue.pop_entity(entity.in_context());
        } else {
            let update = self.get_entity_update(
                entity.in_context(),
                ComponentUpdate::update(DestroyCountdown {
                    remaining: countdown.remaining - frame_data.delta_time(),
                }),
            );
            queue.update_entity(update);
        }
    }
}

impl SpinningData {
    fn new(axis: Vector3, speed: f32) -> Self {
        Self { axis, speed }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MoveData {
    pub direction: Vector3,
    pub speed: f32,
}

type EntityComponent = ecs_context_type![
    Model,
    ShaderHandle,
    Transform,
    SpinningData,
    ProjectionMatrix,
    FirstPersonController,
    SpawnerData,
    MoveData,
    DestroyCountdown,
    Nil
];

struct MoveSystem;

impl System<EntityComponent> for MoveSystem {
    type External = list_type![FrameData, Nil];
    type WriteList = list_type![Transform, Nil];
    type Components = list_type![Transform, MoveData, Nil];

    fn execute<'a>(
        &self,
        entity: EntityIndex,
        unpack_list![transform, move_data]: RefList<'a, Self::Components>,
        _context: &EntityComponent,
        queue: &OperationChannel<'_, EntityComponent>,
        unpack_list![frame_data]: RefList<'a, Self::External>,
    ) {
        let transform = Transform::identity()
            .translate(frame_data.delta_time() * move_data.speed * move_data.direction)
            * *transform;
        let update =
            self.get_entity_update(entity.in_context(), ComponentUpdate::update(transform));
        queue.update_entity(update);
    }
}

struct SpinningSystem;

impl System<EntityComponent> for SpinningSystem {
    type External = list_type![FrameData, Nil];
    type WriteList = list_type![Transform, Nil];
    type Components = list_type![Transform, SpinningData, Nil];

    fn execute<'a>(
        &self,
        entity: EntityIndex,
        unpack_list![transform, spinning_data]: RefList<'a, Self::Components>,
        _context: &EntityComponent,
        queue: &OperationChannel<'_, EntityComponent>,
        unpack_list![frame_data]: RefList<'a, Self::External>,
    ) {
        let transform = Transform::identity().rotate(
            spinning_data.axis,
            frame_data.delta_time() * spinning_data.speed,
        ) * *transform;
        let update =
            self.get_entity_update(entity.in_context(), ComponentUpdate::update(transform));
        queue.update_entity(update);
    }
}

struct SpawnCube;

impl System<EntityComponent> for SpawnCube {
    type External = list_type![InputSystem, Nil];
    type WriteList = Nil;
    type Components = list_type![Transform, SpawnerData, Nil];

    fn execute<'a>(
        &self,
        _entity: EntityIndex,
        unpack_list![transform, spawner]: RefList<'a, Self::Components>,
        _context: &EntityComponent,
        queue: &OperationChannel<'_, EntityComponent>,
        unpack_list![input_system]: RefList<'a, Self::External>,
    ) {
        if input_system
            .get_key_state(Key::Space)
            .matches_state(KeyState::Pressed)
        {
            let new_entity = queue.create_entity();
            queue.add_component(AddComponent::component(new_entity, spawner.mesh));
            queue.add_component(AddComponent::component(new_entity, spawner.shader));
            queue.add_component(AddComponent::component(new_entity, *transform));
            queue.add_component(AddComponent::component(
                new_entity,
                DestroyCountdown { remaining: 5.0 },
            ));
            queue.add_component(AddComponent::component(
                new_entity,
                SpinningData::new(Vector3::new(1.0, 0.0, 0.0), 1.0),
            ));
            let forward = (transform.q * Vector3::x()).norm();
            queue.add_component(AddComponent::component(
                new_entity,
                MoveData {
                    direction: forward,
                    speed: 15.0,
                },
            ));
        }
    }
}

fn handle_input(
    _context: &EntityComponent,
    _queue: &OperationChannel<'_, EntityComponent>,
    input_system: &InputSystem,
    command_queue: &CommandQueue,
) {
    if input_system
        .get_key_state(Key::Q)
        .matches_state(KeyState::Pressed)
    {
        command_queue.send(Command::Quit);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let resolution = Vector2::new(1920.0, 1080.0);
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
        .next_stage::<Parallel>()
        .with_system(DrawCommandSystem)
        .with_system(SpinningSystem)
        .with_system(SpawnCube)
        .next_stage::<Parallel>()
        .with_system(Destroy)
        .with_system(MoveSystem)
        .next_stage::<Synchronous>()
        .with_system(FirstPerson::new::<EntityComponent>())
        .with_global_system(CameraSelector::new::<EntityComponent>())
        .with_global_system(GlobalInput::<EntityComponent, _>::new(handle_input));
    let mut scene = game_loop.scene(renderer_context, systems_context);

    let camera = scene
        .get_entity_builder()
        .with_component(Transform::identity())
        .with_component(FirstPersonController::new(KeyBindings::default(), 4.0, 0.5))
        .with_component(ProjectionMatrix::perspective(
            std::f32::consts::FRAC_PI_3,
            resolution.y / resolution.x,
            1e-1,
            1e4,
        ))
        .with_component(SpawnerData::new(checker_shader.into(), model.into()));
    scene.with_entity(camera);

    (0..10).for_each(|x| {
        (0..10).for_each(|y| {
            (0..10).for_each(|z| {
                let entity =
                    scene
                        .get_entity_builder()
                        .with_component::<ShaderHandle, _>(checker_shader.into())
                        .with_component::<Model, _>(model.clone().into())
                        .with_component::<Transform, _>(Transform::identity().translate(
                            Vector3::new(x as f32 * 3.0, y as f32 * 3.0, z as f32 * 3.0),
                        ))
                        .with_component::<SpinningData, _>(SpinningData::new(
                            Vector3::new(x as f32, y as f32, z as f32).norm(),
                            std::f32::consts::FRAC_PI_2,
                        ));
                scene.with_entity(entity);
            })
        })
    });
    game_loop.run(scene)?;
    Ok(())
}
