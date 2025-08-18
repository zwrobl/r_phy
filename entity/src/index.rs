use std::{
    collections::HashMap,
    fmt::{Debug, Formatter},
    hash::{Hash, Hasher},
};
use type_kit::{FromGuard, GenCollection, GenIndexRaw, GenVec, GenVecIndex, TypeGuard};

use crate::{
    archetype::Archetype,
    context::{EntityComponentContext, EntityType},
};

pub struct PersistentIndexTyped<T: Clone + Copy + Eq + Hash> {
    index: GenVecIndex<T>,
}

impl<T: Clone + Copy + Eq + Hash> Hash for PersistentIndexTyped<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl<T: Clone + Copy + Eq + Hash> PartialEq for PersistentIndexTyped<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T: Clone + Copy + Eq + Hash> Eq for PersistentIndexTyped<T> {}

impl<T: Clone + Copy + Eq + Hash> Clone for PersistentIndexTyped<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + Copy + Eq + Hash> Copy for PersistentIndexTyped<T> {}

impl<T: Clone + Copy + Eq + Hash> Debug for PersistentIndexTyped<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersistentEntityIndexTyped")
            .field("index", &self.index)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PersistentIndex {
    index: TypeGuard<GenIndexRaw>,
}

impl<T: Clone + Copy + Eq + Hash + 'static> From<PersistentIndexTyped<T>> for PersistentIndex {
    #[inline]
    fn from(index: PersistentIndexTyped<T>) -> Self {
        Self {
            index: index.index.into_guard(),
        }
    }
}

impl PersistentIndex {
    #[inline]
    pub fn entity_index<C: EntityComponentContext>(
        &self,
    ) -> PersistentIndexTyped<EntityIndexTyped<C>> {
        let index = GenVecIndex::try_from_guard(self.index).unwrap();
        PersistentIndexTyped { index }
    }
}

#[derive(Debug)]
pub struct PersistentIndexMap<T: Clone + Copy + Eq + Hash + 'static> {
    lookup: HashMap<T, GenVecIndex<T>>,
    items: GenVec<T>,
}

impl<T: Clone + Copy + Eq + Hash + 'static> Default for PersistentIndexMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + Copy + Eq + Hash + 'static> PersistentIndexMap<T> {
    #[inline]
    pub fn new() -> Self {
        Self {
            lookup: HashMap::new(),
            items: GenVec::new(),
        }
    }

    #[inline]
    pub fn register(&mut self, entity: T) {
        if !self.lookup.contains_key(&entity) {
            let index_mapping = self.items.push(entity).unwrap();
            self.lookup.insert(entity, index_mapping);
        }
    }

    #[inline]
    pub fn unregister(&mut self, entity: T) {
        if let Some(index_mapping) = self.lookup.remove(&entity) {
            self.items.pop(index_mapping).unwrap();
        }
    }

    #[inline]
    pub fn update(&mut self, index: PersistentIndexTyped<T>, entity: T) {
        let PersistentIndexTyped { index } = index;
        if let Ok(&registered) = self.items.get(index)
            && registered != entity
        {
            self.items[index] = entity;
            self.lookup.remove(&registered);
            self.lookup.insert(entity, index);
        }
    }

    #[inline]
    pub fn into_iter(&self) -> impl Iterator<Item = &'_ T> {
        (&self.items).into_iter()
    }

    #[inline]
    pub fn contains(&self, entity: T) -> bool {
        self.lookup.contains_key(&entity)
    }

    #[inline]
    pub fn get_index(&self, entity: T) -> PersistentIndexTyped<T> {
        let index = *self.lookup.get(&entity).unwrap();
        PersistentIndexTyped { index }
    }

    #[inline]
    pub fn try_get(&self, index: PersistentIndexTyped<T>) -> Option<T> {
        let PersistentIndexTyped { index } = index;
        self.items.get(index).ok().copied()
    }
}

pub struct EntityIndexTyped<E: EntityComponentContext> {
    pub archetype: GenVecIndex<Archetype<E>>,
    pub entity: GenVecIndex<EntityType<E>>,
}

impl<E: EntityComponentContext> Hash for EntityIndexTyped<E> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.archetype.hash(state);
        self.entity.hash(state);
    }
}

impl<E: EntityComponentContext> PartialEq for EntityIndexTyped<E> {
    fn eq(&self, other: &Self) -> bool {
        self.archetype == other.archetype && self.entity == other.entity
    }
}

impl<E: EntityComponentContext> Eq for EntityIndexTyped<E> {}

impl<E: EntityComponentContext> Clone for EntityIndexTyped<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: EntityComponentContext> Copy for EntityIndexTyped<E> {}

impl<E: EntityComponentContext> Debug for EntityIndexTyped<E> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityIndexTyped")
            .field("archetype", &self.archetype)
            .field("entity", &self.entity)
            .finish()
    }
}

impl<E: EntityComponentContext> EntityIndexTyped<E> {
    pub fn new(archetype: GenVecIndex<Archetype<E>>, entity: GenVecIndex<EntityType<E>>) -> Self {
        Self { archetype, entity }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityIndex {
    pub archetype: TypeGuard<GenIndexRaw>,
    pub entity: TypeGuard<GenIndexRaw>,
}

impl<E: EntityComponentContext> From<EntityIndexTyped<E>> for EntityIndex {
    fn from(index: EntityIndexTyped<E>) -> Self {
        Self {
            archetype: index.archetype.into_guard(),
            entity: index.entity.into_guard(),
        }
    }
}

impl EntityIndex {
    pub fn in_context<C: EntityComponentContext>(&self) -> EntityIndexTyped<C> {
        let archetype = GenVecIndex::try_from_guard(self.archetype).unwrap();
        let entity = GenVecIndex::try_from_guard(self.entity).unwrap();
        EntityIndexTyped { archetype, entity }
    }
}
