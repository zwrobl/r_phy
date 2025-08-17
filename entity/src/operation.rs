use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    marker::PhantomData,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{channel, Receiver, Sender},
    },
};

use type_kit::{Marker, UContains};

use crate::{
    context::{EntityComponentContext, EntityUpdateType, UpdateResult},
    entity::{ComponentUpdate, EntityBuilder, EntityUpdate, UpdatePayload},
    index::EntityIndexTyped,
    ComponentData,
};

pub struct OperationSender<E: EntityComponentContext> {
    push: Sender<AddComponent<E>>,
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

pub struct OperationChannel<'a, E: EntityComponentContext> {
    num_added: &'a AtomicUsize,
    sender: OperationSender<E>,
}

impl<'a, E: EntityComponentContext> Clone for OperationChannel<'a, E> {
    fn clone(&self) -> Self {
        Self {
            num_added: self.num_added,
            sender: self.sender.clone(),
        }
    }
}

pub struct OperationReceiver<E: EntityComponentContext> {
    push: Receiver<AddComponent<E>>,
    pop: Receiver<EntityIndexTyped<E>>,
    update: Receiver<EntityUpdate<E>>,
}

pub struct OperationQueue<E: EntityComponentContext> {
    num_added: AtomicUsize,
    receiver: OperationReceiver<E>,
    sender: Option<OperationSender<E>>,
}

impl<E: EntityComponentContext> OperationQueue<E> {
    pub fn take_channel(&mut self) -> OperationChannel<E> {
        OperationChannel {
            num_added: &self.num_added,
            sender: self.sender.take().unwrap(),
        }
    }
}

pub struct TemporaryIndex<E: EntityComponentContext> {
    index: usize,
    _phantom: PhantomData<E>,
}

pub struct AddComponent<E: EntityComponentContext> {
    index: TemporaryIndex<E>,
    payload: UpdatePayload<E>,
}

impl<E: EntityComponentContext> AddComponent<E> {
    pub fn component<C: ComponentData, M: Marker>(index: TemporaryIndex<E>, component: C) -> Self
    where
        EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
    {
        Self {
            payload: UpdatePayload {
                update: EntityUpdateType::<E>::new(ComponentUpdate::update(component)),
                component: TypeId::of::<C>(),
            },
            index,
        }
    }
}

impl<'a, E: EntityComponentContext> OperationChannel<'a, E> {
    #[inline]
    pub fn create_entity(&self) -> TemporaryIndex<E> {
        // TODO: Handle Overflow
        TemporaryIndex {
            index: self.num_added.fetch_add(1, Ordering::Relaxed),
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn add_component(&self, entity: AddComponent<E>) {
        self.sender.push.send(entity).unwrap();
    }

    #[inline]
    pub fn pop_entity(&self, entity: EntityIndexTyped<E>) {
        self.sender.pop.send(entity).unwrap();
    }

    #[inline]
    pub fn update_entity(&self, update: EntityUpdate<E>) {
        self.sender.update.send(update).unwrap();
    }
}

impl<E: EntityComponentContext> OperationQueue<E> {
    pub fn process(self, world: &mut E) {
        let update_mapper = unsafe { world.get_update_mapper() };
        let mut builders = Vec::with_capacity(self.num_added.load(Ordering::Relaxed));
        rayon::scope(|scope| {
            scope.spawn(|_| {
                let num_added = self.num_added.load(Ordering::Relaxed);
                (0..num_added).for_each(|_| builders.push(EntityBuilder::<E>::new()));

                self.receiver.push.into_iter().for_each(|add| {
                    let index = add.index.index;
                    let builder = builders.get_mut(index).unwrap();
                    update_mapper.update_builder(builder, add.payload);
                });
            });

            scope.spawn(|_| {
                let pop: Vec<_> = self.receiver.pop.into_iter().collect();
                let mut removed = HashSet::with_capacity(pop.len());

                pop.into_iter().for_each(|index| {
                    if world.pop_entity(index).is_some() {
                        removed.insert(index);
                    }
                });

                let update: Vec<_> = self.receiver.update.into_iter().collect();
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
                                    update_mapper.update_builder(builder, update.payload);
                                }
                            }
                            _ => (),
                        }
                    }
                });

                updated
                    .into_iter()
                    .for_each(|(_, (builder, persistent_index))| {
                        world.push_entity(builder, Some(persistent_index));
                    });
            });
        });

        builders
            .into_iter()
            .for_each(|entity| world.push_entity(entity, None));
    }
}

impl<E: EntityComponentContext> OperationQueue<E> {
    pub fn new() -> Self {
        let (push_sender, push_receiver) = channel();
        let (pop_sender, pop_receiver) = channel();
        let (update_sender, update_receiver) = channel();
        Self {
            num_added: AtomicUsize::new(0),
            sender: Some(OperationSender {
                push: push_sender,
                pop: pop_sender,
                update: update_sender,
            }),
            receiver: OperationReceiver {
                push: push_receiver,
                pop: pop_receiver,
                update: update_receiver,
            },
        }
    }
}
