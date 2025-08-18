use std::marker::PhantomData;

use type_kit::{GenCollection, GenVec, GenVecIndex, IntoSubsetIterator, MarkedIndexList, Marker};

use crate::{
    ComponentList, ExternalSystem,
    archetype::{Archetype, ArchetypeMut, ArchetypeRef},
    entity::{
        Entity, EntityBuilder, EntityRef, EntityUpdate, EntityUpdateMapper, Query, QueryWrite,
        UpdateMapWriter, UpdateMapperRef,
    },
    index::{EntityIndexTyped, PersistentIndexMap, PersistentIndexTyped},
    stage::{self, StageListBuilder, Strategy},
};

pub enum UpdateResult<E: EntityComponentContext> {
    ArchetypeChanged((EntityBuilder<E>, PersistentIndexTyped<EntityIndexTyped<E>>)),
    NotFound(EntityUpdate<E>),
    InPlace,
}

pub type EntityType<E> = <E as EntityComponentContext>::Entity;

pub type ComponentListType<E> = <E as EntityComponentContext>::Components;

pub type EntityQueryType<E> = <EntityType<E> as Entity<
    <E as EntityComponentContext>::Components,
    <E as EntityComponentContext>::Marker,
>>::Query;

pub type EntityUpdateType<E> = <EntityType<E> as Entity<
    <E as EntityComponentContext>::Components,
    <E as EntityComponentContext>::Marker,
>>::Update;

pub type EntityBuilderType<E> = <EntityType<E> as Entity<
    <E as EntityComponentContext>::Components,
    <E as EntityComponentContext>::Marker,
>>::Builder;

pub type EntityOwnedType<E> = <EntityType<E> as MarkedIndexList<
    <E as EntityComponentContext>::Components,
    <E as EntityComponentContext>::Marker,
>>::Owned;

pub type EntityRefType<'a, E> = <EntityType<E> as MarkedIndexList<
    <E as EntityComponentContext>::Components,
    <E as EntityComponentContext>::Marker,
>>::Ref<'a>;

pub type EntityMutType<'a, E> = <EntityType<E> as MarkedIndexList<
    <E as EntityComponentContext>::Components,
    <E as EntityComponentContext>::Marker,
>>::Mut<'a>;

pub struct ExternalSystemsSelector<E: EntityComponentContext, C: ExternalSystem> {
    _phantom: PhantomData<(E, C)>,
}

impl<E: EntityComponentContext, C: ExternalSystem> Default for ExternalSystemsSelector<E, C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: EntityComponentContext, C: ExternalSystem> ExternalSystemsSelector<E, C> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }

    pub fn next_stage<T: Strategy<E, C>>(&self) -> impl stage::Builder<E, C> + use<T, E, C> {
        StageListBuilder::<E, C, T, _, _>::new()
    }
}

pub trait EntityComponentContext: Default + Sized + Sync + Send + 'static {
    type Components: ComponentList;
    type Marker: Marker;
    type Entity: Entity<Self::Components, Self::Marker>;

    #[inline]
    fn with_external<E: ExternalSystem>() -> ExternalSystemsSelector<Self, E> {
        ExternalSystemsSelector::new()
    }

    fn push_entity(
        &mut self,
        entity: EntityBuilder<Self>,
        persistent_index: Option<PersistentIndexTyped<EntityIndexTyped<Self>>>,
    );

    fn pop_entity(&mut self, index: EntityIndexTyped<Self>) -> Option<EntityOwnedType<Self>>;

    fn update_entity(&mut self, update: EntityUpdate<Self>) -> UpdateResult<Self>;

    fn iter_ref<'a>(&'a self) -> impl Iterator<Item = ArchetypeRef<'a, Self>>;

    fn iter_mut<'a>(&'a mut self) -> impl Iterator<Item = ArchetypeMut<'a, Self>>;

    fn query<
        'a,
        M2: Marker,
        N: IntoSubsetIterator<Self::Components, M2> + QueryWrite<EntityQueryType<Self>, M2> + 'a,
    >(
        &'a self,
    ) -> impl Iterator<Item = EntityRef<'a, Self, M2, N>>;

    fn try_get_entity<'a>(
        &'a self,
        index: EntityIndexTyped<Self>,
    ) -> Option<EntityRefType<'a, Self>>;

    fn get_persistent_index(
        &self,
        entity: EntityIndexTyped<Self>,
    ) -> PersistentIndexTyped<EntityIndexTyped<Self>>;

    fn try_map_persistent(
        &self,
        index: PersistentIndexTyped<EntityIndexTyped<Self>>,
    ) -> Option<EntityIndexTyped<Self>>;

    fn get_entity_builder(&self) -> EntityBuilder<Self>;

    fn write_update_map<M1: Marker>(&mut self)
    where
        Self::Components: UpdateMapWriter<Self, M1>;

    /// # Safety
    /// Function returns reference to context update mapper without borrowing the context
    /// This can be safely done as long as the owning context is not moved or dropped
    /// because the update mapper is immutable and does not change during the lifetime of the context.
    /// User must ensure that the context outlives the reference,
    unsafe fn get_update_mapper(&self) -> UpdateMapperRef<Self>;
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> EntityComponentContext
    for EntityComponentStorage<C, M, E>
{
    type Components = C;
    type Marker = M;
    type Entity = E;

    fn push_entity(
        &mut self,
        entity: EntityBuilder<Self>,
        persistent_index: Option<PersistentIndexTyped<EntityIndexTyped<Self>>>,
    ) {
        let query = entity.query();
        let archetype = self
            .iter_mut()
            .find(|archetype| archetype.is_matching(&query));
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

    fn pop_entity(&mut self, index: EntityIndexTyped<Self>) -> Option<E::Owned> {
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

    fn update_entity(&mut self, update: EntityUpdate<Self>) -> UpdateResult<Self> {
        if self
            .persistent_archetype_map
            .contains(update.index.archetype)
        {
            let archetype = &mut self.archetypes[update.index.archetype];
            let archetype_changed = self
                .update_mapper
                .archetype_changed(archetype.query(), &update.payload);
            if !archetype_changed {
                if let Some(entity) = archetype.try_get_entity_mut(update.index) {
                    self.update_mapper.update_in_place(entity, update.payload);
                    return UpdateResult::InPlace;
                }
            } else if let Some(mut entity) = archetype.try_pop_entity(update.index) {
                self.update_mapper.update_owned(&mut entity, update.payload);
                let builder = EntityBuilder::from_owned(entity);
                let persistent_index = self.persistent_entity_map.get_index(update.index);
                return UpdateResult::ArchetypeChanged((builder, persistent_index));
            }
        }
        UpdateResult::NotFound(update)
    }

    fn iter_ref<'a>(&'a self) -> impl Iterator<Item = ArchetypeRef<'a, Self>> {
        (&self.archetypes)
            .into_iter()
            .zip(self.persistent_archetype_map.into_iter())
            .map(|(archetype, &index)| archetype.as_ref(index))
    }

    fn iter_mut<'a>(&'a mut self) -> impl Iterator<Item = ArchetypeMut<'a, Self>> {
        (&mut self.archetypes)
            .into_iter()
            .zip(self.persistent_archetype_map.into_iter())
            .map(|(archetype, &index)| archetype.as_mut(index))
    }

    fn query<'a, M2: Marker, N: IntoSubsetIterator<C, M2> + QueryWrite<E::Query, M2> + 'a>(
        &'a self,
    ) -> impl Iterator<Item = EntityRef<'a, Self, M2, N>> {
        let query = N::write(E::Query::default());
        self.iter_ref()
            .filter(move |archetype| query.is_subset(&archetype.query))
            .flat_map(|archetype| archetype.sub_iter_entity())
    }

    fn try_get_entity<'a>(&'a self, index: EntityIndexTyped<Self>) -> Option<E::Ref<'a>> {
        self.persistent_archetype_map
            .contains(index.archetype)
            .then_some(self.archetypes[index.archetype].try_get_entity(index))
            .flatten()
    }

    fn get_persistent_index(
        &self,
        entity: EntityIndexTyped<Self>,
    ) -> PersistentIndexTyped<EntityIndexTyped<Self>> {
        self.persistent_entity_map.get_index(entity)
    }

    fn try_map_persistent(
        &self,
        index: PersistentIndexTyped<EntityIndexTyped<Self>>,
    ) -> Option<EntityIndexTyped<Self>> {
        self.persistent_entity_map.try_get(index)
    }

    fn get_entity_builder(&self) -> EntityBuilder<Self> {
        EntityBuilder::new()
    }

    fn write_update_map<M1: Marker>(&mut self)
    where
        Self::Components: UpdateMapWriter<Self, M1>,
    {
        Self::Components::write_update_map(&mut self.update_mapper);
    }

    unsafe fn get_update_mapper(&self) -> UpdateMapperRef<Self> {
        UpdateMapperRef::new(&self.update_mapper)
    }
}

pub struct EntityComponentStorage<C: ComponentList, M: Marker, E: Entity<C, M>> {
    archetypes: GenVec<Archetype<Self>>,
    persistent_archetype_map: PersistentIndexMap<GenVecIndex<Archetype<Self>>>,
    persistent_entity_map: PersistentIndexMap<EntityIndexTyped<Self>>,
    update_mapper: EntityUpdateMapper<Self>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Default for EntityComponentStorage<C, M, E> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> EntityComponentStorage<C, M, E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            archetypes: GenVec::new(),
            persistent_archetype_map: PersistentIndexMap::new(),
            persistent_entity_map: PersistentIndexMap::new(),
            update_mapper: EntityUpdateMapper::default(),
        }
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
    [$($components:ty),*] => { EntityComponentStorage<component_list_type![$($components),*], marker_type![Here, $($components),*], entity_type![$($components),*]> };
}
