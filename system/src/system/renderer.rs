use std::{
    marker::PhantomData,
    sync::{
        mpsc::{Receiver, Sender},
        Mutex,
    },
};

use entity::{
    context::{ComponentListType, EntityComponentContext, EntityQueryType},
    entity::Expected,
    index::EntityIndex,
    operation::OperationChannel,
    system::{GlobalSystem, System},
};
use graphics::{
    model::Model,
    renderer::{
        camera::{Camera, ProjectionMatrix},
        Context, DrawMapper, RendererContext,
    },
    shader::ShaderHandle,
};
use math::transform::Transform;
use type_kit::{
    list_type, unpack_any, unpack_list, Cons, Contains, Fin, GenVec, Marker, Nil, RefList,
};

#[derive(Clone, Copy)]
pub struct DrawCommand {
    pub shader: ShaderHandle,
    pub model: Model,
    pub transform: Transform,
}

impl DrawCommand {
    pub fn new(shader: ShaderHandle, model: Model, transform: Transform) -> Self {
        Self {
            shader,
            model,
            transform,
        }
    }
}

pub struct DrawQueue {
    sender: Sender<DrawCommand>,
}

impl DrawQueue {
    pub fn push(&self, command: DrawCommand) {
        self.sender.send(command).unwrap()
    }
}
pub struct RenderingSystem<R: RendererContext, M: DrawMapper> {
    renderer: Context<R, M>,
    receiver: Receiver<DrawCommand>,
}

impl<R: RendererContext, M: DrawMapper> RenderingSystem<R, M> {
    pub fn new(renderer: Context<R, M>) -> (DrawQueue, Self) {
        let (sender, receiver) = std::sync::mpsc::channel();
        (DrawQueue { sender }, Self { receiver, renderer })
    }

    pub fn process(&mut self, camera: &CameraCell) {
        let commands: Vec<_> = self.receiver.try_iter().collect();
        if let Some(camera) = camera.take() {
            let _ = self.renderer.begin_frame(&camera.into());
            commands.iter().for_each(|command| {
                self.renderer
                    .draw(command.shader, command.model, &command.transform.into())
                    .unwrap();
            });
            let _ = self.renderer.end_frame();
        }
    }
}

pub struct DrawCommandSystem;

impl<E: EntityComponentContext> System<E> for DrawCommandSystem {
    type External = list_type![DrawQueue, Nil];
    type WriteList = Nil;
    type Components = list_type![Model, ShaderHandle, Transform, Nil];

    fn execute<'a>(
        &self,
        _entity: EntityIndex,
        unpack_list![model, shader, transform]: RefList<'a, Self::Components>,
        _context: &E,
        _queue: &OperationChannel<'_, E>,
        unpack_list![draw_queue]: RefList<'a, Self::External>,
    ) {
        draw_queue.push(DrawCommand::new(*shader, *model, *transform));
    }
}

pub struct CameraCell {
    matrices: Mutex<Option<Camera>>,
}

impl CameraCell {
    pub fn new() -> Self {
        Self {
            matrices: Mutex::new(None),
        }
    }

    pub fn insert(&self, camera: Camera) {
        *self.matrices.lock().unwrap() = Some(camera);
    }

    pub fn take(&self) -> Option<Camera> {
        self.matrices.lock().unwrap().take()
    }
}

pub struct CameraSelector<M: Marker> {
    _marker: PhantomData<M>,
}

impl<M1: Marker, M2: Marker> CameraSelector<Cons<M1, M2>> {
    pub fn new<E: EntityComponentContext>() -> Self
    where
        ComponentListType<E>: Contains<GenVec<ProjectionMatrix>, M1>,
        EntityQueryType<E>: Contains<Expected<ProjectionMatrix>, M1>,
        ComponentListType<E>: Contains<GenVec<Transform>, M2>,
        EntityQueryType<E>: Contains<Expected<Transform>, M2>,
    {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<E: EntityComponentContext, M1: Marker, M2: Marker> GlobalSystem<E>
    for CameraSelector<Cons<M1, M2>>
where
    ComponentListType<E>: Contains<GenVec<ProjectionMatrix>, M1>,
    EntityQueryType<E>: Contains<Expected<ProjectionMatrix>, M1>,
    ComponentListType<E>: Contains<GenVec<Transform>, M2>,
    EntityQueryType<E>: Contains<Expected<Transform>, M2>,
{
    type External = list_type![CameraCell, Nil];
    type WriteList = Nil;

    fn execute<'a>(
        &self,
        context: &E,
        _queue: &OperationChannel<'_, E>,
        unpack_list![camera_cell]: RefList<'a, Self::External>,
    ) {
        if let Some(entity) = context
            .query::<_, list_type![ProjectionMatrix, Fin<Transform>]>()
            .next()
        {
            let unpack_any![camera, transform] = entity.components;
            camera_cell.insert(camera.with_view(*transform));
        }
    }
}
