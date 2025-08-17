use std::{any::TypeId, collections::HashMap, fmt::Debug, marker::PhantomData, ops::Deref};

use type_kit::{
    CollectionType, Cons, Contains, Fin, GenCollectionResult, GenVec, GenVecIndex,
    IntoSubsetIterator, MarkedIndexList, MarkedItemList, Marker, Nil, OptionalList, StaticTypeList,
    TypeList, UCons, UContains, UnionList,
};

use crate::{
    context::{
        ComponentListType, EntityBuilderType, EntityComponentContext, EntityMutType,
        EntityOwnedType, EntityQueryType, EntityType, EntityUpdateType,
    },
    index::EntityIndexTyped,
    Archetype, ComponentData, ComponentList,
};

pub trait Entity<C: ComponentList, M: Marker>:
    MarkedIndexList<C, M> + StaticTypeList + OptionalList + Clone + Copy + Send + Sync
{
    type Query: TypeList + Default + Clone + Copy + Query + Send + Sync;
    type Builder: MarkedItemList<C, M, IndexList = Self> + OptionalList + Default + Send;
    type Update: UnionList + Send;

    fn is_matching(&self, query: &Self::Query) -> bool;

    fn into_builder(value: Self::Owned) -> Self::Builder;

    fn query_from_owned(value: &Self::Owned) -> Self::Query;

    fn query_from_builder(value: &Self::Builder) -> Self::Query;

    fn get_ref<'a>(self, components: &'a C) -> GenCollectionResult<Self::Ref<'a>> {
        <Self as MarkedIndexList<C, M>>::get_ref(self, components)
    }

    fn get_mut<'a>(self, components: &'a mut C) -> GenCollectionResult<Self::Mut<'a>> {
        unsafe { <Self as MarkedIndexList<C, M>>::get_mut(self, components) }
    }

    fn get_owned<'a>(self, components: &'a mut C) -> GenCollectionResult<Self::Owned> {
        <Self as MarkedIndexList<C, M>>::get_owned(self, components)
    }
}

impl<T: ComponentList, M: Marker> Entity<T, M> for Nil
where
    T: Contains<Nil, M>,
{
    type Query = Nil;
    type Builder = Nil;
    type Update = Nil;

    #[inline]
    fn is_matching(&self, _query: &Self::Query) -> bool {
        true
    }

    #[inline]
    fn into_builder(value: Self::Owned) -> Self::Builder {
        value
    }

    #[inline]
    fn query_from_owned(value: &Self::Owned) -> Self::Query {
        *value
    }

    #[inline]
    fn query_from_builder(value: &Self::Builder) -> Self::Query {
        *value
    }
}

impl<C: ComponentData, T: ComponentList, M1: Marker, M2: Marker, N: Entity<T, M2>>
    Entity<T, Cons<M1, M2>> for Cons<Option<GenVecIndex<C>>, N>
where
    T: Contains<GenVec<C>, M1>,
{
    type Query = Cons<Expected<C>, N::Query>;
    type Builder = Cons<Option<CollectionType<C, GenVec<C>>>, N::Builder>;
    type Update = UCons<ComponentUpdate<C>, N::Update>;

    #[inline]
    fn is_matching(&self, query: &Self::Query) -> bool {
        if self.head.is_some() && query.is_expected() {
            self.tail.is_matching(&query.tail)
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
    fn query_from_owned(value: &Self::Owned) -> Self::Query {
        let Cons { head, tail } = value;
        Cons::new(Expected::new(head.is_some()), N::query_from_owned(tail))
    }

    #[inline]
    fn query_from_builder(value: &Self::Builder) -> Self::Query {
        let Cons { head, tail } = value;
        Cons::new(Expected::new(head.is_some()), N::query_from_builder(tail))
    }
}

pub struct ComponentUpdater<E: EntityComponentContext, C: ComponentData, M: Marker>
where
    EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
    EntityQueryType<E>: Contains<Expected<C>, M>,
    EntityOwnedType<E>: Contains<Option<C>, M>,
    EntityBuilderType<E>: Contains<Option<CollectionType<C, GenVec<C>>>, M>,
    for<'a> EntityMutType<'a, E>: Contains<Option<&'a mut C>, M>,
{
    _phatnom: PhantomData<(C, M, E)>,
}

impl<E: EntityComponentContext, C: ComponentData, M: Marker> ComponentUpdater<E, C, M>
where
    EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
    EntityQueryType<E>: Contains<Expected<C>, M>,
    EntityOwnedType<E>: Contains<Option<C>, M>,
    EntityBuilderType<E>: Contains<Option<CollectionType<C, GenVec<C>>>, M>,
    for<'a> EntityMutType<'a, E>: Contains<Option<&'a mut C>, M>,
{
    fn archetype_changed<'a>(archetype: &EntityQueryType<E>, update: &EntityUpdateType<E>) -> bool {
        let expected = archetype.get();
        match unsafe { update.get() } {
            ComponentUpdate::Update(_) => !expected.is_expected(),
            ComponentUpdate::Remove => expected.is_expected(),
            ComponentUpdate::Keep => false,
        }
    }

    fn update_in_place<'a>(mut entity: EntityMutType<'a, E>, update: EntityUpdateType<E>) {
        let entity = entity.get_mut();
        match (unsafe { update.take() }, entity) {
            (ComponentUpdate::Update(component), Some(entity)) => **entity = component,
            _ => (),
        }
    }

    fn update_builder(entity: &mut EntityBuilderType<E>, update: EntityUpdateType<E>) {
        match unsafe { update.take() } {
            ComponentUpdate::Update(component) => {
                *entity.get_mut() = Some(CollectionType::new(component))
            }
            _ => (),
        }
    }

    fn update_owned(entity: &mut EntityOwnedType<E>, update: EntityUpdateType<E>) {
        match unsafe { update.take() } {
            ComponentUpdate::Update(component) => *entity.get_mut() = Some(component),
            _ => (),
        }
    }
}

pub struct EntityUpdateMapper<E: EntityComponentContext> {
    archetype_changed: HashMap<TypeId, fn(&EntityQueryType<E>, &EntityUpdateType<E>) -> bool>,
    update_in_place: HashMap<TypeId, fn(EntityMutType<'_, E>, EntityUpdateType<E>)>,
    update_builder: HashMap<TypeId, fn(&mut EntityBuilderType<E>, EntityUpdateType<E>)>,
    update_owned: HashMap<TypeId, fn(&mut EntityOwnedType<E>, EntityUpdateType<E>)>,
}

pub struct UpdateMapperRef<E: EntityComponentContext> {
    update_mapper: *const EntityUpdateMapper<E>,
}

unsafe impl<E: EntityComponentContext> Send for UpdateMapperRef<E> {}

unsafe impl<E: EntityComponentContext> Sync for UpdateMapperRef<E> {}

impl<'a, E: EntityComponentContext> Deref for UpdateMapperRef<E> {
    type Target = EntityUpdateMapper<E>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.update_mapper }
    }
}

impl<'a, E: EntityComponentContext> UpdateMapperRef<E> {
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
        archetype: &EntityQueryType<E>,
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

    pub fn register<C: ComponentData, M: Marker>(&mut self)
    where
        EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
        EntityQueryType<E>: Contains<Expected<C>, M>,
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
        M1: Marker,
        M2: Marker,
        N: UpdateMapWriter<E, M2>,
    > UpdateMapWriter<E, Cons<M1, M2>> for Cons<GenVec<C>, N>
where
    ComponentListType<E>: Contains<GenVec<C>, M1>,
    EntityUpdateType<E>: UContains<ComponentUpdate<C>, M1>,
    EntityQueryType<E>: Contains<Expected<C>, M1>,
    EntityOwnedType<E>: Contains<Option<C>, M1>,
    EntityBuilderType<E>: Contains<Option<CollectionType<C, GenVec<C>>>, M1>,
    for<'a> EntityMutType<'a, E>: Contains<Option<&'a mut C>, M1>,
{
    fn write_update_map(update_map: &mut EntityUpdateMapper<E>) {
        update_map.register::<C, M1>();
        N::write_update_map(update_map);
    }
}

#[derive(Debug)]
pub struct Expected<C: ComponentData> {
    expected: bool,
    _marker: PhantomData<C>,
}

impl<C: ComponentData> PartialEq for Expected<C> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.expected == other.expected
    }
}

impl<C: ComponentData> Eq for Expected<C> {}

impl<C: ComponentData> Expected<C> {
    #[inline]
    pub fn new(expected: bool) -> Self {
        Self {
            expected,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn is_expected(&self) -> bool {
        self.expected
    }
}

impl<C: ComponentData> Clone for Expected<C> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: ComponentData> Copy for Expected<C> {}

impl<C: ComponentData> Default for Expected<C> {
    #[inline]
    fn default() -> Self {
        Self::new(false)
    }
}

pub trait QueryWrite<Q: TypeList, M: Marker> {
    fn write(query: Q) -> Q;
}

impl<Q: TypeList, M: Marker> QueryWrite<Q, M> for Nil
where
    Q: Contains<Nil, M>,
{
    fn write(query: Q) -> Q {
        query
    }
}

impl<C: ComponentData, Q: TypeList, M: Marker> QueryWrite<Q, M> for Fin<C>
where
    Q: Contains<Expected<C>, M>,
    C: 'static,
{
    fn write(mut query: Q) -> Q {
        *query.get_mut() = Expected::<C>::new(true);
        query
    }
}

impl<Q: TypeList, C: ComponentData, M1: Marker, M2: Marker, N: QueryWrite<Q, M2>>
    QueryWrite<Q, Cons<M1, M2>> for Cons<C, N>
where
    Q: Contains<Expected<C>, M1>,
{
    fn write(mut query: Q) -> Q {
        *query.get_mut() = Expected::<C>::new(true);
        N::write(query)
    }
}

pub trait Query: PartialEq + Eq {
    fn is_subset(self, other: &Self) -> bool;

    fn is_empty(self) -> bool;

    fn get_union(self, other: &Self) -> Self;

    fn get_intersection(self, other: &Self) -> Self;
}

impl Query for Nil {
    #[inline]
    fn is_subset(self, _other: &Self) -> bool {
        true
    }

    fn is_empty(self) -> bool {
        true
    }

    #[inline]
    fn get_union(self, _other: &Self) -> Self {
        self
    }

    fn get_intersection(self, _other: &Self) -> Self {
        self
    }
}

impl<C: ComponentData, N: Query> Query for Cons<Expected<C>, N> {
    #[inline]
    fn is_subset(self, other: &Self) -> bool {
        let valid = if self.head.is_expected() {
            other.head.is_expected()
        } else {
            true
        };
        valid && self.tail.is_subset(&other.tail)
    }

    #[inline]
    fn is_empty(self) -> bool {
        !self.head.is_expected() && self.tail.is_empty()
    }

    #[inline]
    fn get_union(self, other: &Self) -> Self {
        Cons::new(
            Expected::new(self.is_expected() || other.head.is_expected()),
            self.tail.get_union(&other.tail),
        )
    }

    #[inline]
    fn get_intersection(self, other: &Self) -> Self {
        Cons::new(
            Expected::new(self.is_expected() && other.head.is_expected()),
            self.tail.get_intersection(&other.tail),
        )
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

impl<'a, C: ComponentData> From<&'a ComponentUpdate<C>> for Expected<C> {
    #[inline]
    fn from(value: &'a ComponentUpdate<C>) -> Self {
        match value {
            ComponentUpdate::Remove => Expected::new(false),
            _ => Expected::new(true),
        }
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

impl<E: EntityComponentContext> EntityBuilder<E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            // query_builder: EntityQueryType::<E>::default(),
            entity_builder: EntityBuilderType::<E>::default(),
        }
    }

    #[inline]
    pub fn from_owned(entity: EntityOwnedType<E>) -> Self {
        Self {
            // query_builder: EntityType::<E>::query_from_owned(&entity),
            entity_builder: EntityType::<E>::into_builder(entity),
        }
    }

    #[inline]
    pub fn with_component<C: ComponentData, M2: Marker>(self, component: C) -> Self
    where
        EntityBuilderType<E>: Contains<Option<CollectionType<C, GenVec<C>>>, M2>,
        EntityQueryType<E>: Contains<Expected<C>, M2>,
    {
        let Self {
            mut entity_builder,
            // mut query_builder,
        } = self;
        *entity_builder.get_mut() = Some(CollectionType::new(component));
        // *query_builder.get_mut() = Expected::new(true);
        Self {
            // query_builder,
            entity_builder,
        }
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
