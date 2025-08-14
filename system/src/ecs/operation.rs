use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::{channel, Receiver, Sender},
};

use type_kit::{Marker, TypeList};

use crate::ecs::{
    context::{EntityComponentConfiguration, EntityComponentContext, UpdateResult},
    entity::{Entity, EntityBuilder, EntityUpdate, EntityUpdateBuilder},
    index::EntityIndexTyped,
    ComponentList,
};

pub enum Operation<C: ComponentList, M: Marker, E: Entity<C, M>> {
    Push(EntityBuilder<C, M, E>),
    Pop(EntityIndexTyped<C, M, E>),
    Update(EntityUpdate<C, M, E>),
}

pub struct OperationSender<C: ComponentList, M: Marker, E: Entity<C, M>> {
    sender: Sender<Operation<C, M, E>>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Clone for OperationSender<C, M, E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> OperationSender<C, M, E> {
    #[inline]
    pub fn push_entity(&self, entity: EntityBuilder<C, M, E>) {
        self.sender.send(Operation::Push(entity)).unwrap();
    }

    #[inline]
    pub fn pop_entity(&self, entity: EntityIndexTyped<C, M, E>) {
        self.sender.send(Operation::Pop(entity)).unwrap();
    }

    #[inline]
    pub fn update_entity<W: TypeList>(&self, entity: EntityUpdateBuilder<C, M, E, W>) {
        self.sender.send(Operation::Update(entity.build())).unwrap();
    }
}

pub struct OperationReceiver<C: ComponentList, M: Marker, E: Entity<C, M>> {
    receiver: Receiver<Operation<C, M, E>>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> OperationReceiver<C, M, E> {
    pub fn process(self, world: &mut EntityComponentContext<C, M, E>) {
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
                                    builder.update_components(update.components);
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
    pub fn new<C: ComponentList, M: Marker, E: Entity<C, M>>(
    ) -> (OperationSender<C, M, E>, OperationReceiver<C, M, E>) {
        let (sender, receiver) = channel();
        (OperationSender { sender }, OperationReceiver { receiver })
    }
}

pub type ContextQueue<C> = OperationSender<
    <C as EntityComponentConfiguration>::Components,
    <C as EntityComponentConfiguration>::Marker,
    <C as EntityComponentConfiguration>::Entity,
>;
