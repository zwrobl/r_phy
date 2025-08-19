use std::{
    any::TypeId,
    collections::HashMap,
    fmt::Debug,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::Deref,
};

use type_kit::{
    CollectionType, Cons, Contains, Fin, GenCollectionResult, GenVec, GenVecIndex, IndexedMarker,
    IntoSubsetIterator, MarkedIndexList, MarkedItemList, Marker, Nil, NonEmptyList, OptionalList,
    StaticTypeList, UCons, UContains, UnionList,
};

use crate::{
    Archetype, ComponentData, ComponentList,
    context::{
        ComponentListType, EntityBuilderType, EntityComponentContext, EntityMutType,
        EntityOwnedType, EntityQueryType, EntityType, EntityUpdateType,
    },
    index::EntityIndexTyped,
};

pub trait Entity<C: ComponentList, M: Marker>:
    MarkedIndexList<C, M> + StaticTypeList + OptionalList + Clone + Copy + Send + Sync
{
    type Builder: MarkedItemList<C, M, IndexList = Self> + OptionalList + Default + Send;
    type Update: UnionList + Send;

    fn is_matching(&self, query: Query<C>) -> bool;

    fn into_builder(value: Self::Owned) -> Self::Builder;

    fn query_from_owned(value: &Self::Owned) -> Query<C>;

    fn query_from_builder(value: &Self::Builder) -> Query<C>;

    fn get_ref(self, components: &C) -> GenCollectionResult<Self::Ref<'_>> {
        <Self as MarkedIndexList<C, M>>::get_ref(self, components)
    }

    fn get_mut(self, components: &mut C) -> GenCollectionResult<Self::Mut<'_>> {
        unsafe { <Self as MarkedIndexList<C, M>>::get_mut(self, components) }
    }

    fn get_owned(self, components: &mut C) -> GenCollectionResult<Self::Owned> {
        <Self as MarkedIndexList<C, M>>::get_owned(self, components)
    }
}

impl<T: ComponentList, M: Marker> Entity<T, M> for Nil
where
    T: Contains<Nil, M>,
{
    type Builder = Nil;
    type Update = Nil;

    #[inline]
    fn is_matching(&self, _query: Query<T>) -> bool {
        true
    }

    #[inline]
    fn into_builder(value: Self::Owned) -> Self::Builder {
        value
    }

    #[inline]
    fn query_from_owned(_value: &Self::Owned) -> Query<T> {
        Query::empty()
    }

    #[inline]
    fn query_from_builder(_value: &Self::Builder) -> Query<T> {
        Query::empty()
    }
}

impl<C: ComponentData, T: ComponentList, M1: IndexedMarker, M2: Marker, N: Entity<T, M2>>
    Entity<T, Cons<M1, M2>> for Cons<Option<GenVecIndex<C>>, N>
where
    T: Contains<GenVec<C>, M1>,
{
    type Builder = Cons<Option<CollectionType<C, GenVec<C>>>, N::Builder>;
    type Update = UCons<ComponentUpdate<C>, N::Update>;

    #[inline]
    fn is_matching(&self, query: Query<T>) -> bool {
        if self.head.is_some() && query.has_component() {
            self.tail.is_matching(query)
        } else {
            false
        }
    }

    #[inline]
    fn into_builder(value: Self::Owned) -> Self::Builder {
        let Cons { head, tail } = value;
        Cons::new(
            head.map(|value| CollectionType::new(value)),
            N::into_builder(tail),
        )
    }

    #[inline]
    fn query_from_owned(value: &Self::Owned) -> Query<T> {
        let Cons { head, tail } = value;
        let query = N::query_from_owned(tail);
        if head.is_some() {
            query.with_component()
        } else {
            query
        }
    }

    #[inline]
    fn query_from_builder(value: &Self::Builder) -> Query<T> {
        let Cons { head, tail } = value;
        let query = N::query_from_builder(tail);
        if head.is_some() {
            query.with_component()
        } else {
            query
        }
    }
}

pub struct ComponentUpdater<E: EntityComponentContext, C: ComponentData, M: IndexedMarker>
where
    ComponentListType<E>: Contains<GenVec<C>, M>,
    EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
    EntityOwnedType<E>: Contains<Option<C>, M>,
    EntityBuilderType<E>: Contains<Option<CollectionType<C, GenVec<C>>>, M>,
    for<'a> EntityMutType<'a, E>: Contains<Option<&'a mut C>, M>,
{
    _phatnom: PhantomData<(C, M, E)>,
}

impl<E: EntityComponentContext, C: ComponentData, M: IndexedMarker> ComponentUpdater<E, C, M>
where
    ComponentListType<E>: Contains<GenVec<C>, M>,
    EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
    EntityOwnedType<E>: Contains<Option<C>, M>,
    EntityBuilderType<E>: Contains<Option<CollectionType<C, GenVec<C>>>, M>,
    for<'a> EntityMutType<'a, E>: Contains<Option<&'a mut C>, M>,
{
    fn archetype_changed(archetype: EntityQueryType<E>, update: &EntityUpdateType<E>) -> bool {
        let expected = archetype.has_component();
        match unsafe { update.get() } {
            ComponentUpdate::Update(_) => !expected,
            ComponentUpdate::Remove => expected,
            ComponentUpdate::Keep => false,
        }
    }

    fn update_in_place(mut entity: EntityMutType<'_, E>, update: EntityUpdateType<E>) {
        let entity = entity.get_mut();
        if let (ComponentUpdate::Update(component), Some(entity)) =
            (unsafe { update.take() }, entity)
        {
            **entity = component
        }
    }

    fn update_builder(entity: &mut EntityBuilderType<E>, update: EntityUpdateType<E>) {
        if let ComponentUpdate::Update(component) = unsafe { update.take() } {
            *entity.get_mut() = Some(CollectionType::new(component))
        }
    }

    fn update_owned(entity: &mut EntityOwnedType<E>, update: EntityUpdateType<E>) {
        if let ComponentUpdate::Update(component) = unsafe { update.take() } {
            *entity.get_mut() = Some(component)
        }
    }
}

type ArchetypeChangedMap<E> = HashMap<TypeId, fn(EntityQueryType<E>, &EntityUpdateType<E>) -> bool>;
type UpdateInPlaceMap<E> = HashMap<TypeId, fn(EntityMutType<'_, E>, EntityUpdateType<E>)>;
type UpdateBuilderMap<E> = HashMap<TypeId, fn(&mut EntityBuilderType<E>, EntityUpdateType<E>)>;
type UpdateOwnedMap<E> = HashMap<TypeId, fn(&mut EntityOwnedType<E>, EntityUpdateType<E>)>;

pub struct EntityUpdateMapper<E: EntityComponentContext> {
    archetype_changed: ArchetypeChangedMap<E>,
    update_in_place: UpdateInPlaceMap<E>,
    update_builder: UpdateBuilderMap<E>,
    update_owned: UpdateOwnedMap<E>,
}

pub struct UpdateMapperRef<E: EntityComponentContext> {
    update_mapper: *const EntityUpdateMapper<E>,
}

unsafe impl<E: EntityComponentContext> Send for UpdateMapperRef<E> {}

unsafe impl<E: EntityComponentContext> Sync for UpdateMapperRef<E> {}

impl<E: EntityComponentContext> Deref for UpdateMapperRef<E> {
    type Target = EntityUpdateMapper<E>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.update_mapper }
    }
}

impl<E: EntityComponentContext> UpdateMapperRef<E> {
    pub fn new(update_mapper: &EntityUpdateMapper<E>) -> Self {
        Self { update_mapper }
    }
}

impl<E: EntityComponentContext> Default for EntityUpdateMapper<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: EntityComponentContext> EntityUpdateMapper<E> {
    pub fn new() -> Self {
        Self {
            archetype_changed: HashMap::new(),
            update_in_place: HashMap::new(),
            update_builder: HashMap::new(),
            update_owned: HashMap::new(),
        }
    }

    pub fn archetype_changed(
        &self,
        archetype: EntityQueryType<E>,
        update: &UpdatePayload<E>,
    ) -> bool {
        if let Some(&func) = self.archetype_changed.get(&update.component) {
            func(archetype, &update.update)
        } else {
            panic!(
                "No function registered for component: {:?}",
                update.component
            );
        }
    }

    pub fn update_in_place(&self, entity: EntityMutType<'_, E>, update: UpdatePayload<E>) {
        if let Some(&func) = self.update_in_place.get(&update.component) {
            func(entity, update.update);
        } else {
            panic!(
                "No function registered for component: {:?}",
                update.component
            );
        }
    }

    pub fn update_builder(&self, entity: &mut EntityBuilder<E>, update: UpdatePayload<E>) {
        if let Some(&func) = self.update_builder.get(&update.component) {
            func(&mut entity.entity_builder, update.update);
        } else {
            panic!(
                "No function registered for component: {:?}",
                update.component
            );
        }
    }

    pub fn update_owned(&self, entity: &mut EntityOwnedType<E>, update: UpdatePayload<E>) {
        if let Some(&func) = self.update_owned.get(&update.component) {
            func(entity, update.update);
        } else {
            panic!(
                "No function registered for component: {:?}",
                update.component
            );
        }
    }

    pub fn register<C: ComponentData, M: IndexedMarker>(&mut self)
    where
        ComponentListType<E>: Contains<GenVec<C>, M>,
        EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
        EntityOwnedType<E>: Contains<Option<C>, M>,
        EntityBuilderType<E>: Contains<Option<CollectionType<C, GenVec<C>>>, M>,
        for<'a> EntityMutType<'a, E>: Contains<Option<&'a mut C>, M>,
    {
        self.archetype_changed.insert(
            TypeId::of::<C>(),
            ComponentUpdater::<E, C, M>::archetype_changed,
        );
        self.update_in_place.insert(
            TypeId::of::<C>(),
            ComponentUpdater::<E, C, M>::update_in_place,
        );
        self.update_builder.insert(
            TypeId::of::<C>(),
            ComponentUpdater::<E, C, M>::update_builder,
        );
        self.update_owned
            .insert(TypeId::of::<C>(), ComponentUpdater::<E, C, M>::update_owned);
    }
}

pub trait UpdateMapWriter<E: EntityComponentContext, M: Marker> {
    fn write_update_map(update_map: &mut EntityUpdateMapper<E>);
}

impl<E: EntityComponentContext, M: Marker> UpdateMapWriter<E, M> for Nil
where
    ComponentListType<E>: Contains<Nil, M>,
{
    fn write_update_map(_update_map: &mut EntityUpdateMapper<E>) {}
}

impl<
    E: EntityComponentContext,
    C: ComponentData,
    M1: IndexedMarker,
    M2: Marker,
    N: UpdateMapWriter<E, M2>,
> UpdateMapWriter<E, Cons<M1, M2>> for Cons<GenVec<C>, N>
where
    ComponentListType<E>: Contains<GenVec<C>, M1>,
    EntityUpdateType<E>: UContains<ComponentUpdate<C>, M1>,
    EntityOwnedType<E>: Contains<Option<C>, M1>,
    EntityBuilderType<E>: Contains<Option<CollectionType<C, GenVec<C>>>, M1>,
    for<'a> EntityMutType<'a, E>: Contains<Option<&'a mut C>, M1>,
{
    fn write_update_map(update_map: &mut EntityUpdateMapper<E>) {
        update_map.register::<C, M1>();
        N::write_update_map(update_map);
    }
}

pub struct Query<E: ComponentList> {
    component_bitmask: u128,
    _phantom: PhantomData<E>,
}

impl<E: ComponentList> Default for Query<E> {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

impl<E: ComponentList> Clone for Query<E> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: ComponentList> Copy for Query<E> {}

impl<E: ComponentList> PartialEq for Query<E> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.component_bitmask == other.component_bitmask
    }
}

impl<E: ComponentList> Eq for Query<E> {}

impl<E: ComponentList> Hash for Query<E> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.component_bitmask.hash(state);
    }
}

#[cfg(test)]
mod test_entity_query {
    use type_kit::list_type;

    use super::*;

    type ComponentList = list_type![
        GenVec<f64>,
        GenVec<f32>,
        GenVec<u16>,
        GenVec<u32>,
        GenVec<u64>,
        GenVec<u128>,
        Nil
    ];

    #[test]
    fn test_query() {
        let query =
            Query::<ComponentList>::from_component_list::<list_type![f64, u16, u128, Nil], _>();
        assert_eq!(query.component_bitmask, 0b100101);
        let query = Query::<ComponentList>::from_component_list::<
            list_type![f64, f32, u16, u32, u64, u128, Nil],
            _,
        >();
        assert_eq!(query.component_bitmask, 0b111111);
        let query = Query::<ComponentList>::from_component_list::<list_type![u128, Nil], _>();
        assert_eq!(query.component_bitmask, 0b100000);
        let query = Query::<ComponentList>::from_component_list::<list_type![f64, Nil], _>();
        assert_eq!(query.component_bitmask, 0b000001);
    }

    #[test]
    fn test_query_using_fin() {
        let query = Query::<ComponentList>::from_component_list::<Fin<u128>, _>();
        assert_eq!(query.component_bitmask, 0b100000);
        let query = Query::<ComponentList>::from_component_list::<Fin<u64>, _>();
        assert_eq!(query.component_bitmask, 0b010000);
        let query = Query::<ComponentList>::from_component_list::<Fin<u32>, _>();
        assert_eq!(query.component_bitmask, 0b001000);
        let query = Query::<ComponentList>::from_component_list::<Fin<f64>, _>();
        assert_eq!(query.component_bitmask, 0b000001);
    }
}

pub trait ComponentQuery<E: ComponentList, M: Marker> {
    const COMPONENT_BITMASK: u128;

    fn query() -> Query<E> {
        Query {
            component_bitmask: Self::COMPONENT_BITMASK,
            _phantom: PhantomData,
        }
    }
}

impl<E: ComponentList, M: Marker> ComponentQuery<E, M> for Nil
where
    E: Contains<Nil, M>,
{
    const COMPONENT_BITMASK: u128 = 0;
}

impl<C: ComponentData, E: ComponentList, M: IndexedMarker> ComponentQuery<E, M> for Fin<C>
where
    E: Contains<GenVec<C>, M>,
{
    const COMPONENT_BITMASK: u128 = 1 << M::INDEX;
}

impl<C: ComponentData, E: ComponentList, M1: IndexedMarker, M2: Marker, N: ComponentQuery<E, M2>>
    ComponentQuery<E, Cons<M1, M2>> for Cons<C, N>
where
    E: Contains<GenVec<C>, M1>,
{
    const COMPONENT_BITMASK: u128 = 1 << M1::INDEX | N::COMPONENT_BITMASK;
}

impl<E: ComponentList> Query<E> {
    #[inline]
    pub fn empty() -> Self {
        Self {
            component_bitmask: 0,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn from_component_list<L: NonEmptyList, M: Marker>() -> Self
    where
        L: ComponentQuery<E, M>,
    {
        Self {
            component_bitmask: L::COMPONENT_BITMASK,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn is_matching(self, other: Self) -> bool {
        self.component_bitmask == other.component_bitmask
    }

    #[inline]
    pub fn is_subset(self, other: Self) -> bool {
        self.component_bitmask & other.component_bitmask == self.component_bitmask
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.component_bitmask == 0
    }

    #[inline]
    pub fn get_union(mut self, other: Self) -> Self {
        self.component_bitmask |= other.component_bitmask;
        self
    }

    #[inline]
    pub fn get_intersection(mut self, other: Self) -> Self {
        self.component_bitmask &= other.component_bitmask;
        self
    }

    #[inline]
    pub fn with_component<C: ComponentData, M: IndexedMarker>(mut self) -> Self
    where
        E: Contains<GenVec<C>, M>,
    {
        self.component_bitmask |= 1 << M::INDEX;
        self
    }

    #[inline]
    pub fn has_component<C: ComponentData, M: IndexedMarker>(self) -> bool
    where
        E: Contains<GenVec<C>, M>,
    {
        (self.component_bitmask & 1 << M::INDEX) != 0
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ComponentUpdate<C: ComponentData> {
    Update(C),
    Remove,
    Keep,
}

impl<C: ComponentData> ComponentUpdate<C> {
    #[inline]
    pub fn update(component: C) -> Self {
        Self::Update(component)
    }

    #[inline]
    pub fn remove() -> Self {
        Self::Remove
    }

    #[inline]
    pub fn keep() -> Self {
        Self::Keep
    }
}

impl<C: ComponentData> Default for ComponentUpdate<C> {
    #[inline]
    fn default() -> Self {
        Self::Keep
    }
}

pub struct UpdatePayload<E: EntityComponentContext> {
    pub update: EntityUpdateType<E>,
    pub component: TypeId,
}

pub struct EntityUpdate<E: EntityComponentContext> {
    pub index: EntityIndexTyped<E>,
    pub payload: UpdatePayload<E>,
}

impl<E: EntityComponentContext> EntityUpdate<E> {
    #[inline]
    pub fn new<C: ComponentData, M: Marker>(
        index: EntityIndexTyped<E>,
        component: ComponentUpdate<C>,
    ) -> Self
    where
        EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
    {
        Self {
            index,
            payload: UpdatePayload {
                update: EntityUpdateType::<E>::new(component),
                component: TypeId::of::<C>(),
            },
        }
    }
}

pub struct EntityRef<
    'a,
    E: EntityComponentContext,
    M2: Marker,
    N: IntoSubsetIterator<ComponentListType<E>, M2> + 'a,
> {
    pub index: EntityIndexTyped<E>,
    pub components: N::RefList<'a>,
    _marker: PhantomData<M2>,
}

impl<
    'a,
    E: EntityComponentContext,
    M2: Marker,
    N: IntoSubsetIterator<ComponentListType<E>, M2> + 'a,
> EntityRef<'a, E, M2, N>
{
    pub fn new(
        archetype: GenVecIndex<Archetype<E>>,
        entity: GenVecIndex<EntityType<E>>,
        components: N::RefList<'a>,
    ) -> Self {
        Self {
            index: EntityIndexTyped::new(archetype, entity),
            components,
            _marker: PhantomData,
        }
    }
}

pub struct EntityBuilder<E: EntityComponentContext> {
    pub entity_builder: EntityBuilderType<E>,
}

impl<E: EntityComponentContext> Default for EntityBuilder<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: EntityComponentContext> EntityBuilder<E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            entity_builder: EntityBuilderType::<E>::default(),
        }
    }

    #[inline]
    pub fn from_owned(entity: EntityOwnedType<E>) -> Self {
        Self {
            entity_builder: EntityType::<E>::into_builder(entity),
        }
    }

    #[inline]
    pub fn with_component<C: ComponentData, M2: Marker>(mut self, component: C) -> Self
    where
        ComponentListType<E>: Contains<GenVec<C>, M2>,
        EntityBuilderType<E>: Contains<Option<CollectionType<C, GenVec<C>>>, M2>,
    {
        *self.entity_builder.get_mut() = Some(CollectionType::new(component));
        self
    }

    #[inline]
    pub fn build(self) -> EntityBuilderType<E> {
        self.entity_builder
    }

    #[inline]
    pub fn query(&self) -> EntityQueryType<E> {
        EntityType::<E>::query_from_builder(&self.entity_builder)
    }
}
