use std::{fmt::Debug, marker::PhantomData, ops::Deref};

use type_kit::{
    CollectionType, Cons, Contains, GenVec, GenVecIndex, IntoSubsetIterator, MarkedIndexList,
    MarkedItemList, Marker, Nil, OptionalList, StaticTypeList, TypeList,
};

use crate::ecs::{index::EntityIndexTyped, Archetype, ComponentData, ComponentList};

pub trait Entity<C: ComponentList, M: Marker>:
    MarkedIndexList<C, M> + StaticTypeList + OptionalList + Clone + Copy + Send + Sync
{
    type Query: TypeList + Default + Clone + Copy + Query + Send + Sync;
    type Builder: MarkedItemList<C, M, IndexList = Self> + OptionalList + Default + Send;
    type Update: Default + Send;

    fn is_matching(&self, query: &Self::Query) -> bool;

    fn into_builder(value: Self::Owned) -> Self::Builder;

    fn query_from_owned(value: &Self::Owned) -> Self::Query;

    fn query_from_builder(value: &Self::Builder) -> Self::Query;

    fn query_from_update(value: &Self::Update) -> Self::Query;

    fn update_owned(value: &mut Self::Owned, update: Self::Update);

    fn update_builder(value: &mut Self::Builder, update: Self::Update);

    fn update_in_place<'a>(value: Self::Mut<'a>, update: Self::Update);
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

    #[inline]
    fn query_from_update(value: &Self::Update) -> Self::Query {
        *value
    }

    #[inline]
    fn update_owned(_value: &mut Self::Owned, _update: Self::Update) {}

    #[inline]
    fn update_builder(_value: &mut Self::Builder, _update: Self::Update) {}

    #[inline]
    fn update_in_place<'a>(_value: Self::Mut<'a>, _update: Self::Update) {}
}

impl<C: ComponentData, T: ComponentList, M1: Marker, M2: Marker, N: Entity<T, M2>>
    Entity<T, Cons<M1, M2>> for Cons<Option<GenVecIndex<C>>, N>
where
    T: Contains<GenVec<C>, M1>,
{
    type Query = Cons<Expected<C>, N::Query>;
    type Builder = Cons<Option<CollectionType<C, GenVec<C>>>, N::Builder>;
    type Update = Cons<ComponentUpdate<C>, N::Update>;

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

    #[inline]
    fn query_from_update(value: &Self::Update) -> Self::Query {
        let Cons { head, tail } = value;
        Cons::new(head.into(), N::query_from_update(tail))
    }

    #[inline]
    fn update_owned(value: &mut Self::Owned, update: Self::Update) {
        match update.head {
            ComponentUpdate::Update(component) => value.head = Some(component),
            ComponentUpdate::Remove => value.head = None,
            ComponentUpdate::Keep => (),
        }
        N::update_owned(&mut value.tail, update.tail);
    }

    #[inline]
    fn update_builder(value: &mut Self::Builder, update: Self::Update) {
        match update.head {
            ComponentUpdate::Update(component) => value.head = Some(CollectionType::new(component)),
            ComponentUpdate::Remove => value.head = None,
            ComponentUpdate::Keep => (),
        }
        N::update_builder(&mut value.tail, update.tail);
    }

    #[inline]
    fn update_in_place<'a>(value: Self::Mut<'a>, update: Self::Update) {
        if let (ComponentUpdate::Update(component), Some(value)) = (update.head, value.head) {
            *value = component;
        }
        N::update_in_place(value.tail, update.tail);
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

pub enum ComponentUpdate<C: ComponentData> {
    Update(C),
    Remove,
    Keep,
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

pub struct EntityUpdateBuilder<C: ComponentList, M1: Marker, E: Entity<C, M1>, W: TypeList> {
    index: EntityIndexTyped<C, M1, E>,
    components: E::Update,
    _phantom: PhantomData<W>,
}

impl<C: ComponentList, M1: Marker, E: Entity<C, M1>, W: TypeList> EntityUpdateBuilder<C, M1, E, W> {
    #[inline]
    pub fn new(index: EntityIndexTyped<C, M1, E>) -> Self {
        Self {
            index,
            components: E::Update::default(),
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn update<C2: ComponentData, M2: Marker, M3: Marker>(mut self, component: C2) -> Self
    where
        E::Update: Contains<ComponentUpdate<C2>, M2>,
        W: Contains<C2, M3>,
    {
        *self.components.get_mut() = ComponentUpdate::Update(component);
        self
    }

    #[inline]
    pub fn remove<C2: ComponentData, M2: Marker, M3: Marker>(mut self) -> Self
    where
        E::Update: Contains<ComponentUpdate<C2>, M2>,
        W: Contains<C2, M3>,
    {
        *self.components.get_mut() = ComponentUpdate::Remove;
        self
    }

    #[inline]
    pub fn build(self) -> EntityUpdate<C, M1, E> {
        EntityUpdate {
            index: self.index,
            components: self.components,
        }
    }
}

pub struct EntityUpdate<C: ComponentList, M: Marker, E: Entity<C, M>> {
    pub index: EntityIndexTyped<C, M, E>,
    pub components: E::Update,
}

pub struct EntityRef<
    'a,
    C: ComponentList,
    M1: Marker,
    M2: Marker,
    E: Entity<C, M1>,
    N: IntoSubsetIterator<C, M2> + 'a,
> {
    pub index: EntityIndexTyped<C, M1, E>,
    pub components: N::RefList<'a>,
    _marker: PhantomData<M2>,
}

impl<
        'a,
        C: ComponentList,
        M1: Marker,
        M2: Marker,
        E: Entity<C, M1>,
        N: IntoSubsetIterator<C, M2> + 'a,
    > EntityRef<'a, C, M1, M2, E, N>
{
    pub fn new(
        archetype: GenVecIndex<Archetype<C, M1, E>>,
        entity: GenVecIndex<E>,
        components: N::RefList<'a>,
    ) -> Self {
        Self {
            index: EntityIndexTyped::new(archetype, entity),
            components,
            _marker: PhantomData,
        }
    }
}

pub struct EntityBuilder<T: ComponentList, M: Marker, E: Entity<T, M>> {
    pub query_builder: E::Query,
    entity_builder: E::Builder,
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> Deref for EntityBuilder<T, M, E> {
    type Target = E::Query;

    fn deref(&self) -> &Self::Target {
        &self.query_builder
    }
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> EntityBuilder<T, M, E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            query_builder: E::Query::default(),
            entity_builder: E::Builder::default(),
        }
    }

    #[inline]
    pub fn from_owned(entity: E::Owned) -> Self {
        Self {
            query_builder: E::query_from_owned(&entity),
            entity_builder: E::into_builder(entity),
        }
    }

    #[inline]
    pub fn with_component<C: ComponentData, M2: Marker>(self, component: C) -> Self
    where
        E::Builder: Contains<Option<CollectionType<C, GenVec<C>>>, M2>,
        E::Query: Contains<Expected<C>, M2>,
    {
        let Self {
            mut entity_builder,
            mut query_builder,
        } = self;
        *entity_builder.get_mut() = Some(CollectionType::new(component));
        *query_builder.get_mut() = Expected::new(true);
        Self {
            query_builder,
            entity_builder,
        }
    }

    #[inline]
    pub fn update_components(&mut self, components: E::Update) {
        E::update_builder(&mut self.entity_builder, components);
        self.query_builder = E::query_from_builder(&self.entity_builder);
    }

    #[inline]
    pub fn build(self) -> E::Builder {
        self.entity_builder
    }
}
