use type_kit::{GenCollection, GenVec, GenVecIndex, IntoSubsetIterator, Marker, Nil};

use crate::ecs::{
    archetype::{Archetype, ArchetypeMut, ArchetypeRef},
    entity::{
        Entity, EntityBuilder, EntityRef, EntityUpdate, EntityUpdateBuilder, Query, QueryWrite,
    },
    index::EntityIndexTyped,
    index::{PersistentIndexMap, PersistentIndexTyped},
    system::{StageListBuilder, System},
    ComponentList, ExternalSystem,
};

pub enum UpdateResult<C: ComponentList, M: Marker, E: Entity<C, M>> {
    ArchetypeChanged(
        (
            EntityBuilder<C, M, E>,
            PersistentIndexTyped<EntityIndexTyped<C, M, E>>,
        ),
    ),
    NotFound(EntityUpdate<C, M, E>),
    InPlace,
}

pub trait EntityComponentConfiguration {
    type Components: ComponentList;
    type Marker: Marker;
    type Entity: Entity<Self::Components, Self::Marker>;
    type Context;

    #[inline]
    fn with_external<E: ExternalSystem>(
        _: &E,
    ) -> StageListBuilder<Self::Components, E, Self::Marker, Self::Entity, Nil, Nil> {
        StageListBuilder::new()
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> EntityComponentConfiguration
    for EntityComponentContext<C, M, E>
{
    type Components = C;
    type Marker = M;
    type Entity = E;
    type Context = Self;
}

pub struct EntityComponentContext<C: ComponentList, M: Marker, E: Entity<C, M>> {
    archetypes: GenVec<Archetype<C, M, E>>,
    persistent_archetype_map: PersistentIndexMap<GenVecIndex<Archetype<C, M, E>>>,
    persistent_entity_map: PersistentIndexMap<EntityIndexTyped<C, M, E>>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Default for EntityComponentContext<C, M, E> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> EntityComponentContext<C, M, E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            archetypes: GenVec::new(),
            persistent_archetype_map: PersistentIndexMap::new(),
            persistent_entity_map: PersistentIndexMap::new(),
        }
    }

    pub fn push_entity(
        &mut self,
        entity: EntityBuilder<C, M, E>,
        persistent_index: Option<PersistentIndexTyped<EntityIndexTyped<C, M, E>>>,
    ) {
        let archetype = self
            .iter_mut()
            .find(|archetype| archetype.is_matching(&entity));
        let entity = match archetype {
            Some(mut archetype) => archetype.push_entity(entity),
            None => {
                let archetype = self.archetypes.push(Archetype::new()).unwrap();
                self.persistent_archetype_map.register(archetype);
                self.archetypes[archetype]
                    .as_mut(archetype)
                    .set_archetype(entity)
            }
        };
        if let Some(persistent_index) = persistent_index {
            self.persistent_entity_map.update(persistent_index, entity);
        } else {
            self.persistent_entity_map.register(entity);
        }
    }

    pub fn pop_entity(&mut self, index: EntityIndexTyped<C, M, E>) -> Option<E::Owned> {
        let removed = self
            .persistent_archetype_map
            .contains(index.archetype)
            .then_some(self.archetypes[index.archetype].try_pop_entity(index))
            .flatten();
        if removed.is_some() {
            self.persistent_entity_map.unregister(index);
        }
        removed
    }

    pub fn update_entity(&mut self, update: EntityUpdate<C, M, E>) -> UpdateResult<C, M, E> {
        if self
            .persistent_archetype_map
            .contains(update.index.archetype)
        {
            let archetype = &mut self.archetypes[update.index.archetype];
            if archetype.is_matching(&E::query_from_update(&update.components)) {
                if let Some(entity) = archetype.try_get_entity_mut(update.index) {
                    E::update_in_place(entity, update.components);
                    return UpdateResult::InPlace;
                }
            } else {
                if let Some(mut entity) = archetype.try_pop_entity(update.index) {
                    E::update_owned(&mut entity, update.components);
                    let builder = EntityBuilder::from_owned(entity);
                    let persistent_index = self.persistent_entity_map.get_index(update.index);
                    return UpdateResult::ArchetypeChanged((builder, persistent_index));
                }
            }
        }
        UpdateResult::NotFound(update)
    }

    pub fn iter_ref<'a>(&'a self) -> impl Iterator<Item = ArchetypeRef<'a, C, M, E>> {
        (&self.archetypes)
            .into_iter()
            .zip(self.persistent_archetype_map.into_iter())
            .map(|(archetype, &index)| archetype.as_ref(index))
    }

    fn iter_mut<'a>(&'a mut self) -> impl Iterator<Item = ArchetypeMut<'a, C, M, E>> {
        (&mut self.archetypes)
            .into_iter()
            .zip(self.persistent_archetype_map.into_iter())
            .map(|(archetype, &index)| archetype.as_mut(index))
    }

    pub fn query<'a, M2: Marker, N: IntoSubsetIterator<C, M2> + QueryWrite<E::Query, M2> + 'a>(
        &'a self,
    ) -> impl Iterator<Item = EntityRef<'a, C, M, M2, E, N>> {
        let query = N::write(E::Query::default());
        self.iter_ref()
            .filter(move |archetype| query.is_subset(&archetype.query))
            .flat_map(|archetype| archetype.sub_iter_entity())
    }

    pub fn try_get_entity<'a>(&'a self, index: EntityIndexTyped<C, M, E>) -> Option<E::Ref<'a>> {
        self.persistent_archetype_map
            .contains(index.archetype)
            .then_some(self.archetypes[index.archetype].try_get_entity(index))
            .flatten()
    }

    pub fn get_persistent_index(
        &self,
        entity: EntityIndexTyped<C, M, E>,
    ) -> PersistentIndexTyped<EntityIndexTyped<C, M, E>> {
        self.persistent_entity_map.get_index(entity)
    }

    pub fn try_map_persistent(
        &self,
        index: PersistentIndexTyped<EntityIndexTyped<C, M, E>>,
    ) -> Option<EntityIndexTyped<C, M, E>> {
        self.persistent_entity_map.try_get(index)
    }

    pub fn get_entity_builder(&self) -> EntityBuilder<C, M, E> {
        EntityBuilder::new()
    }

    pub fn get_entity_update_builder<S: System<Self>>(
        &self,
        _system: &S,
        index: EntityIndexTyped<C, M, E>,
    ) -> EntityUpdateBuilder<C, M, E, S::WriteList> {
        EntityUpdateBuilder::new(index)
    }
}

#[macro_export]
macro_rules! component_list_type {
    [$component:ty, $last:ty] => { Cons<GenVec<$component>, $last>   };
    [$component:ty $(, $components:ty)*] => {
        Cons<GenVec<$component>, component_list_type![$($components),*]>
    };
}

#[macro_export]
macro_rules! marker_type {
    [$current_marker:ty, $component:ty, $($rest:ty),*] => {
        Cons<$current_marker, marker_type!( There<$current_marker>, $($rest),* )>
    };
    [$current_marker:ty, $component:ty] => {
        $current_marker
    };
}

#[macro_export]
macro_rules! entity_type {
    [$component:ty, $last:ty] => { Cons<Option<GenVecIndex<$component>>, $last> };
    [$component:ty $(, $components:ty)*] => {
        Cons<Option<GenVecIndex<$component>>, entity_type![$($components),*]>
    };
}

#[macro_export]
macro_rules! ecs_context_type {
    [$($components:ty),*] => { EntityComponentContext<component_list_type![$($components),*], marker_type![Here, $($components),*], entity_type![$($components),*]> };
}
