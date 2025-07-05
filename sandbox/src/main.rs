use graphics::{
    model::{CommonVertex, EmptyMaterial, Model, PbrMaterial, SimpleVertex, UnlitMaterial},
    shader::Shader,
};
use std::{error::Error, result::Result};
use vulkan::{
    context::device::renderer::deferred::{DeferredRenderer, DeferredShader},
    VulkanContextBuilder, VulkanRendererBuilder, VulkanRendererConfig,
};
use winit::{
    dpi::PhysicalSize,
    window::{WindowBuilder, WindowButtons},
};

use graphics::renderer::camera::first_person::FirstPersonCameraBuilder;
use math::{
    transform::Transform,
    types::{Matrix4, Vector3},
};
use physics::shape::Cube;
use system::{LoopBuilder, Object};

const RENDERER_MEM_ALLOC_PAGE_SIZE: usize = 128 * 1024 * 1024;

fn main() -> Result<(), Box<dyn Error>> {
    let renderer_builder = VulkanRendererBuilder::<DeferredRenderer>::new().with_config(
        VulkanRendererConfig::builder()
            .with_page_size(RENDERER_MEM_ALLOC_PAGE_SIZE)
            .build()?,
    );
    let proj = Matrix4::perspective(std::f32::consts::FRAC_PI_3, 600.0 / 800.0, 1e-3, 1e3);
    let window_builder = WindowBuilder::new()
        .with_inner_size(PhysicalSize {
            width: 800,
            height: 600,
        })
        .with_resizable(false)
        .with_enabled_buttons(WindowButtons::CLOSE | WindowButtons::MINIMIZE)
        .with_title("r_phy")
        .with_transparent(false);
    let camera_builder = FirstPersonCameraBuilder::new(proj);
    let game_loop = LoopBuilder::new()
        .with_window(window_builder)
        .with_renderer(renderer_builder)
        .with_camera(camera_builder)
        .build()?;
    let mut context_builder = VulkanContextBuilder::new()
        .with_material_type::<UnlitMaterial>()
        .with_material_type::<PbrMaterial>()
        .with_material_type::<EmptyMaterial>()
        .with_mesh_type::<CommonVertex>()
        .with_mesh_type::<SimpleVertex>()
        .with_shader_type::<DeferredShader<Shader<CommonVertex, EmptyMaterial>>>()
        .with_shader_type::<DeferredShader<Shader<CommonVertex, UnlitMaterial>>>()
        .with_shader_type::<DeferredShader<Shader<CommonVertex, PbrMaterial>>>();
    let empty_material = context_builder.add_material(EmptyMaterial::default());
    let cube_mesh = context_builder.add_mesh::<CommonVertex, _>(Cube::new(1.0f32).into());
    // TODO: Explicit type conversion to the type used by selected renderer should not be visible at the front-end
    let checker_shader = context_builder.add_shader::<DeferredShader<_>, _>(
        Shader::<CommonVertex, EmptyMaterial>::new(
            "_resources/shaders/spv/deferred/gbuffer_write/checker",
        )
        .into(),
    );
    let scene = game_loop.scene(context_builder)?.with_objects(
        checker_shader,
        vec![
            Object::new(
                Model::new(cube_mesh, empty_material),
                Transform::identity().translate(Vector3::new(4.0, 0.0, 0.0)),
                Box::new(|elapsed_time, transform| {
                    Transform::identity()
                        .rotate(Vector3::z(), elapsed_time * std::f32::consts::FRAC_PI_2)
                        * transform
                }),
            ),
            Object::new(
                Model::new(cube_mesh, empty_material),
                Transform::identity().translate(Vector3::new(4.0, 2.0, 0.0)),
                Box::new(|elapsed_time, transform| {
                    Transform::identity()
                        .rotate(Vector3::z(), elapsed_time * std::f32::consts::FRAC_PI_2)
                        * transform
                }),
            ),
        ],
    );
    game_loop.run(scene)?;
    Ok(())
}
