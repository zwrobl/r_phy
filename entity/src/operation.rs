use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::{channel, Receiver, Sender},
};

use crate::{
    context::{EntityComponentContext, UpdateResult},
    entity::{EntityBuilder, EntityUpdate},
    index::EntityIndexTyped,
};

pub enum Operation<E: EntityComponentContext> {
    Push(EntityBuilder<E>),
    Pop(EntityIndexTyped<E>),
    Update(EntityUpdate<E>),
}

pub struct OperationSender<E: EntityComponentContext> {
    sender: Sender<Operation<E>>,
}

impl<E: EntityComponentContext> Clone for OperationSender<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<E: EntityComponentContext> OperationSender<E> {
    #[inline]
    pub fn push_entity(&self, entity: EntityBuilder<E>) {
        self.sender.send(Operation::Push(entity)).unwrap();
    }

    #[inline]
    pub fn pop_entity(&self, entity: EntityIndexTyped<E>) {
        self.sender.send(Operation::Pop(entity)).unwrap();
    }

    #[inline]
    pub fn update_entity(&self, update: EntityUpdate<E>) {
        self.sender.send(Operation::Update(update)).unwrap();
    }
}

pub struct OperationReceiver<E: EntityComponentContext> {
    receiver: Receiver<Operation<E>>,
}

impl<E: EntityComponentContext> OperationReceiver<E> {
    pub fn process(self, world: &mut E) {
        let operations: Vec<_> = self.receiver.into_iter().collect();

        let mut updated = HashMap::with_capacity(operations.len());
        let mut removed = HashSet::with_capacity(operations.len());
        operations
            .into_iter()
            .for_each(|operation| match operation {
                Operation::Push(entity) => world.push_entity(entity, None),

                Operation::Pop(index) => {
                    if world.pop_entity(index).is_some() {
                        removed.insert(index);
                    } else if updated.contains_key(&index) {
                        updated.remove(&index);
                        removed.insert(index);
                    }
                }
                Operation::Update(update) => {
                    let index = update.index;
                    if !removed.contains(&index) {
                        match world.update_entity(update) {
                            UpdateResult::ArchetypeChanged(builder) => {
                                updated.insert(index, builder);
                            }
                            UpdateResult::NotFound(update) => {
                                if let Some((builder, ..)) = updated.get_mut(&index) {
                                    world.update_entity_builder(builder, update);
                                }
                            }
                            _ => (),
                        }
                    }
                }
            });
        updated
            .into_iter()
            .for_each(|(_, (builder, persistent_index))| {
                world.push_entity(builder, Some(persistent_index));
            });
    }
}

pub struct OperationChannel {}

impl OperationChannel {
    pub fn new<E: EntityComponentContext>() -> (OperationSender<E>, OperationReceiver<E>) {
        let (sender, receiver) = channel();
        (OperationSender { sender }, OperationReceiver { receiver })
    }
}
