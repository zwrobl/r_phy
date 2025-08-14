use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use type_kit::{
    GenCollection, GenVec, GenVecIndex, IntoSubsetIterator, ListIter, MarkedItemList, Marker,
};

use crate::ecs::{
    entity::{Entity, EntityBuilder, EntityRef},
    index::EntityIndexTyped,
    ComponentList, PersistentIndexMap,
};

pub struct ArchetypeRef<'a, T: ComponentList, M: Marker, E: Entity<T, M>> {
    archetype: &'a Archetype<T, M, E>,
    index: GenVecIndex<Archetype<T, M, E>>,
}

impl<'a, T: ComponentList, M: Marker, E: Entity<T, M>> Deref for ArchetypeRef<'a, T, M, E> {
    type Target = Archetype<T, M, E>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.archetype
    }
}

impl<'a, T: ComponentList, M: Marker, E: Entity<T, M>> ArchetypeRef<'a, T, M, E> {
    #[inline]
    pub fn sub_iter_entity<M2: Marker, N: IntoSubsetIterator<T, M2> + 'a>(
        self,
    ) -> impl Iterator<Item = EntityRef<'a, T, M, M2, E, N>> {
        // Entity components and its corresponding entity index are pushed/removed into the collections
        // in the same order, this should result in them being stored at the same index in GenVec internal storage
        // thus is safe to assume that zip will yield the correct pairs
        self.archetype
            .sub_iter::<_, N>()
            .zip((self.archetype.persistent_entity_map.into_iter()).into_iter())
            .map(move |(components, &entity)| EntityRef::new(self.index, entity, components))
    }
}

impl<'a, T: ComponentList, M: Marker, E: Entity<T, M>> Clone for ArchetypeRef<'a, T, M, E> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T: ComponentList, M: Marker, E: Entity<T, M>> Copy for ArchetypeRef<'a, T, M, E> {}

pub struct ArchetypeMut<'a, T: ComponentList, M: Marker, E: Entity<T, M>> {
    archetype: &'a mut Archetype<T, M, E>,
    index: GenVecIndex<Archetype<T, M, E>>,
}

impl<'a, T: ComponentList, M: Marker, E: Entity<T, M>> ArchetypeMut<'a, T, M, E> {
    #[inline]
    pub fn push_entity(&mut self, entity: EntityBuilder<T, M, E>) -> EntityIndexTyped<T, M, E> {
        let entity = entity.build();
        let entity = entity.insert(&mut self.components).unwrap();
        let index = self.entities.push(entity).unwrap();
        self.persistent_entity_map.register(index);
        EntityIndexTyped::new(self.index, index)
    }

    #[inline]
    pub fn set_archetype(&mut self, entity: EntityBuilder<T, M, E>) -> EntityIndexTyped<T, M, E> {
        if self.entities.is_empty() {
            self.query = entity.query_builder;
            self.push_entity(entity)
        } else {
            panic!("Cannot set archetype for non-empty archetype");
        }
    }
}

impl<'a, T: ComponentList, M: Marker, E: Entity<T, M>> Deref for ArchetypeMut<'a, T, M, E> {
    type Target = Archetype<T, M, E>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.archetype
    }
}

impl<'a, T: ComponentList, M: Marker, E: Entity<T, M>> DerefMut for ArchetypeMut<'a, T, M, E> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.archetype
    }
}

#[derive(Debug)]
pub struct Archetype<T: ComponentList, M: Marker, E: Entity<T, M>> {
    pub query: E::Query,
    entities: GenVec<E>,
    persistent_entity_map: PersistentIndexMap<GenVecIndex<E>>,
    components: T,
    _marker: PhantomData<M>,
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> Default for Archetype<T, M, E> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> Archetype<T, M, E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            query: E::Query::default(),
            entities: GenVec::new(),
            persistent_entity_map: PersistentIndexMap::new(),
            components: T::default(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn as_ref(&self, index: GenVecIndex<Self>) -> ArchetypeRef<T, M, E> {
        ArchetypeRef {
            archetype: self,
            index,
        }
    }

    #[inline]
    pub fn as_mut(&mut self, index: GenVecIndex<Self>) -> ArchetypeMut<T, M, E> {
        ArchetypeMut {
            archetype: self,
            index,
        }
    }

    #[inline]
    pub fn is_matching(&self, query: &E::Query) -> bool {
        self.query == *query
    }

    #[inline]
    pub fn sub_iter<'a, M2: Marker, N: IntoSubsetIterator<T, M2> + 'a>(
        &'a self,
    ) -> impl Iterator<Item = N::RefList<'a>> {
        ListIter::iter_sub::<_, _, N>(&self.components)
            .all()
            .map(|entity| N::unwrap_ref(entity))
    }

    pub fn try_pop_entity<'a>(&'a mut self, index: EntityIndexTyped<T, M, E>) -> Option<E::Owned> {
        if self.persistent_entity_map.contains(index.entity) {
            let entity = self.entities.pop(index.entity).ok()?;
            let components = entity.get_owned(&mut self.components).ok()?;
            self.persistent_entity_map.unregister(index.entity);
            Some(components)
        } else {
            None
        }
    }

    pub fn try_get_entity<'a>(&'a self, index: EntityIndexTyped<T, M, E>) -> Option<E::Ref<'a>> {
        if self.persistent_entity_map.contains(index.entity) {
            let entity = self.entities.get(index.entity).ok()?;
            let components = entity.get_ref(&self.components).ok()?;
            Some(components)
        } else {
            None
        }
    }

    pub fn try_get_entity_mut<'a>(
        &'a mut self,
        index: EntityIndexTyped<T, M, E>,
    ) -> Option<E::Mut<'a>> {
        if self.persistent_entity_map.contains(index.entity) {
            let entity = self.entities.get(index.entity).ok()?;
            let components = unsafe { entity.get_mut(&mut self.components).ok()? };
            Some(components)
        } else {
            None
        }
    }
}
