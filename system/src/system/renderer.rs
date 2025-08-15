use std::sync::mpsc::{Receiver, Sender};

use entity::{
    context::EntityComponentContext, index::EntityIndex, operation::OperationSender, system::System,
};
use graphics::{model::Model, shader::ShaderHandle};
use math::{transform::Transform, types::Matrix4};
use type_kit::{list_type, unpack_list, Cons, Nil, TypeList};

#[derive(Clone, Copy)]
pub struct DrawCommand {
    pub shader: ShaderHandle,
    pub model: Model,
    pub transform: Matrix4,
}

impl DrawCommand {
    pub fn new(shader: ShaderHandle, model: Model, transform: Matrix4) -> Self {
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
pub struct DrawCommandChannel {
    receiver: Receiver<DrawCommand>,
}

impl DrawCommandChannel {
    pub fn new() -> (DrawQueue, Self) {
        let (sender, receiver) = std::sync::mpsc::channel();
        (DrawQueue { sender }, Self { receiver })
    }

    pub fn receive(&self) -> Vec<DrawCommand> {
        self.receiver.try_iter().collect()
    }
}

pub struct RenderingSystem;

impl<E: EntityComponentContext> System<E> for RenderingSystem {
    type External = list_type![DrawQueue, Nil];
    type WriteList = Nil;
    type Components = list_type![Model, ShaderHandle, Transform, Nil];

    fn execute<'a>(
        &self,
        _entity: EntityIndex,
        unpack_list![model, shader, transform]: <Self::Components as TypeList>::RefList<'a>,
        _context: &E,
        _queue: &OperationSender<E>,
        unpack_list![draw_queue]: <Self::External as TypeList>::RefList<'a>,
    ) {
        draw_queue.push(DrawCommand::new(*shader, *model, (*transform).into()));
    }
}
