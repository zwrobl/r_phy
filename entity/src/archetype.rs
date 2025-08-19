use std::ops::{Deref, DerefMut};

use type_kit::{
    GenCollection, GenVec, GenVecIndex, IntoSubsetIterator, ListIter, MarkedItemList, Marker,
};

use crate::{
    PersistentIndexMap,
    context::{
        ComponentListType, EntityComponentContext, EntityMutType, EntityOwnedType, EntityQueryType,
        EntityRefType, EntityType,
    },
    entity::{Entity, EntityBuilder, EntityRef, Query},
    index::EntityIndexTyped,
};

pub struct ArchetypeRef<'a, E: EntityComponentContext> {
    archetype: &'a Archetype<E>,
    index: GenVecIndex<Archetype<E>>,
}

impl<'a, E: EntityComponentContext> Deref for ArchetypeRef<'a, E> {
    type Target = Archetype<E>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.archetype
    }
}

impl<'a, E: EntityComponentContext> ArchetypeRef<'a, E> {
    #[inline]
    pub fn sub_iter_entity<M2: Marker, N: IntoSubsetIterator<ComponentListType<E>, M2> + 'a>(
        self,
    ) -> impl Iterator<Item = EntityRef<'a, E, M2, N>> {
        // Entity components and its corresponding entity index are pushed/removed into the collections
        // in the same order, this should result in them being stored at the same index in GenVec internal storage
        // thus is safe to assume that zip will yield the correct pairs
        self.archetype
            .sub_iter::<_, N>()
            .zip(self.archetype.persistent_entity_map.into_iter())
            .map(move |(components, &entity)| EntityRef::new(self.index, entity, components))
    }
}

impl<'a, E: EntityComponentContext> Clone for ArchetypeRef<'a, E> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, E: EntityComponentContext> Copy for ArchetypeRef<'a, E> {}

pub struct ArchetypeMut<'a, E: EntityComponentContext> {
    archetype: &'a mut Archetype<E>,
    index: GenVecIndex<Archetype<E>>,
}

impl<'a, E: EntityComponentContext> ArchetypeMut<'a, E> {
    #[inline]
    pub fn push_entity(&mut self, entity: EntityBuilder<E>) -> EntityIndexTyped<E> {
        let entity = entity.build();
        let entity = entity.insert(&mut self.components).unwrap();
        let index = self.entities.push(entity).unwrap();
        self.persistent_entity_map.register(index);
        EntityIndexTyped::new(self.index, index)
    }

    #[inline]
    pub fn set_archetype(&mut self, entity: EntityBuilder<E>) -> EntityIndexTyped<E> {
        if self.entities.is_empty() {
            self.query = entity.query();
            self.push_entity(entity)
        } else {
            panic!("Cannot set archetype for non-empty archetype");
        }
    }
}

impl<'a, E: EntityComponentContext> Deref for ArchetypeMut<'a, E> {
    type Target = Archetype<E>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.archetype
    }
}

impl<'a, E: EntityComponentContext> DerefMut for ArchetypeMut<'a, E> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.archetype
    }
}

pub struct Archetype<E: EntityComponentContext> {
    pub query: EntityQueryType<E>,
    entities: GenVec<EntityType<E>>,
    persistent_entity_map: PersistentIndexMap<GenVecIndex<EntityType<E>>>,
    components: ComponentListType<E>,
}

impl<E: EntityComponentContext> Default for Archetype<E> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<E: EntityComponentContext> Archetype<E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            query: Query::default(),
            entities: GenVec::new(),
            persistent_entity_map: PersistentIndexMap::new(),
            components: Default::default(),
        }
    }

    #[inline]
    pub fn as_ref(&self, index: GenVecIndex<Self>) -> ArchetypeRef<'_, E> {
        ArchetypeRef {
            archetype: self,
            index,
        }
    }

    #[inline]
    pub fn as_mut(&mut self, index: GenVecIndex<Self>) -> ArchetypeMut<'_, E> {
        ArchetypeMut {
            archetype: self,
            index,
        }
    }

    #[inline]
    pub fn is_matching(&self, query: EntityQueryType<E>) -> bool {
        self.query.is_matching(query)
    }

    #[inline]
    pub fn query(&self) -> EntityQueryType<E> {
        self.query
    }

    #[inline]
    pub fn sub_iter<'a, M2: Marker, N: IntoSubsetIterator<ComponentListType<E>, M2> + 'a>(
        &'a self,
    ) -> impl Iterator<Item = N::RefList<'a>> {
        ListIter::iter_sub::<_, _, N>(&self.components)
            .all()
            .map(|entity| N::unwrap_ref(entity))
    }

    pub fn try_pop_entity(&mut self, index: EntityIndexTyped<E>) -> Option<EntityOwnedType<E>> {
        if self.persistent_entity_map.contains(index.entity) {
            let entity = self.entities.pop(index.entity).ok()?;
            let components = entity.get_owned(&mut self.components).ok()?;
            self.persistent_entity_map.unregister(index.entity);
            Some(components)
        } else {
            None
        }
    }

    pub fn try_get_entity<'a>(
        &'a self,
        index: EntityIndexTyped<E>,
    ) -> Option<EntityRefType<'a, E>> {
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
        index: EntityIndexTyped<E>,
    ) -> Option<EntityMutType<'a, E>> {
        if self.persistent_entity_map.contains(index.entity) {
            let entity = self.entities.get(index.entity).ok()?;
            let components = entity.get_mut(&mut self.components).ok()?;
            Some(components)
        } else {
            None
        }
    }
}
