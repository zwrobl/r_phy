use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::{channel, Receiver, Sender},
};

use crate::{
    context::{EntityComponentContext, UpdateResult},
    entity::{EntityBuilder, EntityUpdate},
    index::EntityIndexTyped,
};

pub struct OperationSender<E: EntityComponentContext> {
    push: Sender<EntityBuilder<E>>,
    pop: Sender<EntityIndexTyped<E>>,
    update: Sender<EntityUpdate<E>>,
}

impl<E: EntityComponentContext> Clone for OperationSender<E> {
    fn clone(&self) -> Self {
        Self {
            push: self.push.clone(),
            pop: self.pop.clone(),
            update: self.update.clone(),
        }
    }
}

impl<E: EntityComponentContext> OperationSender<E> {
    #[inline]
    pub fn push_entity(&self, entity: EntityBuilder<E>) {
        self.push.send(entity).unwrap();
    }

    #[inline]
    pub fn pop_entity(&self, entity: EntityIndexTyped<E>) {
        self.pop.send(entity).unwrap();
    }

    #[inline]
    pub fn update_entity(&self, update: EntityUpdate<E>) {
        self.update.send(update).unwrap();
    }
}

pub struct OperationReceiver<E: EntityComponentContext> {
    push: Receiver<EntityBuilder<E>>,
    pop: Receiver<EntityIndexTyped<E>>,
    update: Receiver<EntityUpdate<E>>,
}

impl<E: EntityComponentContext> OperationReceiver<E> {
    pub fn process(self, world: &mut E) {
        let pop: Vec<_> = self.pop.into_iter().collect();
        let mut removed = HashSet::with_capacity(pop.len());

        pop.into_iter().for_each(|index| {
            if world.pop_entity(index).is_some() {
                removed.insert(index);
            }
        });

        let update: Vec<_> = self.update.into_iter().collect();
        let mut updated = HashMap::with_capacity(update.len());

        update.into_iter().for_each(|update| {
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
        });

        self.push
            .into_iter()
            .for_each(|entity| world.push_entity(entity, None));

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
        let (push_sender, push_receiver) = channel();
        let (pop_sender, pop_receiver) = channel();
        let (update_sender, update_receiver) = channel();
        (
            OperationSender {
                push: push_sender,
                pop: pop_sender,
                update: update_sender,
            },
            OperationReceiver {
                push: push_receiver,
                pop: pop_receiver,
                update: update_receiver,
            },
        )
    }
}
