use std::{
    collections::HashSet,
    fmt::{Debug, Formatter},
    marker::PhantomData,
    ops::Deref,
};

use type_kit::{
    CollectionType, Cons, Contains, FromGuard, GenCollection, GenIndexRaw, GenVec, GenVecIndex,
    IntoCollectionIterator, IntoSubsetIterator, ListIter, MarkedIndexList, MarkedItemList, Marker,
    Nil, TypeGuard, TypeList,
};

pub trait System<T: EntityComponentConfiguration>: 'static {
    type Components: TypeList;

    fn execute<'a>(
        &self,
        components: <Self::Components as TypeList>::RefList<'a>,
        context: &T::Context,
        queue: &mut ContextQueue<T>,
    );
}

pub trait ComponentList: IntoCollectionIterator {}

impl ComponentList for Nil {}

impl<C: 'static, N: ComponentList> ComponentList for Cons<GenVec<C>, N> {}

pub struct SystemExecutor<
    L: ComponentList,
    M1: Marker,
    M2: Marker,
    E: Entity<L, M1>,
    S: System<EntityComponentContext<L, M1, E>>,
> where
    S::Components: IntoSubsetIterator<L, M2>,
{
    query: E::Query,
    system: S,
    _phantom: std::marker::PhantomData<(L, M1, M2)>,
}

impl<
        L: ComponentList,
        M1: Marker,
        M2: Marker,
        E: Entity<L, M1>,
        S: System<EntityComponentContext<L, M1, E>>,
    > SystemExecutor<L, M1, M2, E, S>
where
    S::Components: IntoSubsetIterator<L, M2>,
{
    #[inline]
    pub fn new<M3: Marker>(system: S) -> Self
    where
        S::Components: QueryWrite<E::Query, M3>,
    {
        Self {
            query: S::Components::write(E::Query::default()),
            system,
            _phantom: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn execute<'a, 'b>(
        &'a self,
        archetype: &'b Archetype<L, M1, E>,
        context: &EntityComponentContext<L, M1, E>,
        operation_queue: &mut OperationQueue<L, M1, E>,
    ) {
        if self.is_matching(archetype) {
            archetype.sub_iter::<_, S::Components>().for_each(|entity| {
                self.system.execute(entity, context, operation_queue);
            });
        }
    }

    #[inline]
    pub fn is_matching(&self, archetype: &Archetype<L, M1, E>) -> bool {
        self.query.is_subset(&archetype.query)
    }
}

pub trait SystemList<T: ComponentList, M: Marker, E: Entity<T, M>> {
    fn execute<'a>(
        &'a self,
        archetype: &Archetype<T, M, E>,
        context: &EntityComponentContext<T, M, E>,
        operation_queue: &mut OperationQueue<T, M, E>,
    );
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> SystemList<T, M, E> for Nil {
    fn execute<'a>(
        &'a self,
        _archetype: &Archetype<T, M, E>,
        _context: &EntityComponentContext<T, M, E>,
        _operation_queue: &mut OperationQueue<T, M, E>,
    ) {
    }
}

impl<
        L: ComponentList,
        M1: Marker,
        M2: Marker,
        E: Entity<L, M1>,
        S: System<EntityComponentContext<L, M1, E>>,
        N: SystemList<L, M1, E>,
    > SystemList<L, M1, E> for Cons<SystemExecutor<L, M1, M2, E, S>, N>
where
    S::Components: IntoSubsetIterator<L, M2>,
{
    fn execute(
        &self,
        archetype: &Archetype<L, M1, E>,
        context: &EntityComponentContext<L, M1, E>,
        operation_queue: &mut OperationQueue<L, M1, E>,
    ) {
        self.head.execute(archetype, context, operation_queue);
        self.tail.execute(archetype, context, operation_queue);
    }
}

#[derive(Debug)]
pub struct Expected<T: 'static> {
    expected: bool,
    _marker: PhantomData<T>,
}

impl<T: 'static> PartialEq for Expected<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.expected == other.expected
    }
}

impl<T: 'static> Eq for Expected<T> {}

impl<T: 'static> Expected<T> {
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

impl<T: 'static> Clone for Expected<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for Expected<T> {}

impl<T: 'static> Default for Expected<T> {
    #[inline]
    fn default() -> Self {
        Self::new(false)
    }
}

pub trait QueryWrite<Q: 'static, M: Marker> {
    fn write(query: Q) -> Q;
}

impl<Q: 'static, M: Marker> QueryWrite<Q, M> for Nil
where
    Q: Contains<Nil, M>,
{
    fn write(query: Q) -> Q {
        query
    }
}

impl<Q: 'static, C: 'static, M1: Marker, M2: Marker, N: QueryWrite<Q, M2>>
    QueryWrite<Q, Cons<M1, M2>> for Cons<C, N>
where
    Q: Contains<Expected<C>, M1>,
{
    fn write(mut query: Q) -> Q {
        *query.get_mut() = Expected::<C>::new(true);
        N::write(query)
    }
}

pub struct QueryBuilder<T: ComponentList, M: Marker, E: Entity<T, M>> {
    query: E::Query,
    _marker: PhantomData<(T, M, E)>,
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> Deref for QueryBuilder<T, M, E> {
    type Target = E::Query;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.query
    }
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> Default for QueryBuilder<T, M, E> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ComponentList, M1: Marker, E: Entity<T, M1>> QueryBuilder<T, M1, E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            query: E::Query::default(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn build(self) -> E::Query {
        self.query
    }

    #[inline]
    pub fn with_expected<C: 'static, M2: Marker>(mut self) -> QueryBuilder<T, M1, E>
    where
        E::Query: Contains<Expected<C>, M2>,
    {
        *self.query.get_mut() = Expected::<C>::new(true);
        self
    }
}

pub trait Query: PartialEq + Eq {
    fn is_subset(self, other: &Self) -> bool;
}

impl Query for Nil {
    #[inline]
    fn is_subset(self, _other: &Self) -> bool {
        true
    }
}

impl<C: 'static, N: Query> Query for Cons<Expected<C>, N> {
    #[inline]
    fn is_subset(self, other: &Self) -> bool {
        let valid = if self.head.is_expected() {
            other.head.is_expected()
        } else {
            true
        };
        valid && self.tail.is_subset(&other.tail)
    }
}

pub trait Entity<C: ComponentList, M: Marker>:
    MarkedIndexList<C, M> + Clone + Copy + 'static
{
    type Query: Default + Clone + Copy + 'static + Query;
    type Builder: MarkedItemList<C, M, IndexList = Self> + Default;

    fn is_matching(&self, query: &Self::Query) -> bool;

    #[inline]
    fn query() -> QueryBuilder<C, M, Self> {
        QueryBuilder::new()
    }
}

impl<T: ComponentList, M: Marker> Entity<T, M> for Nil
where
    T: Contains<Nil, M>,
{
    type Query = Nil;
    type Builder = Nil;

    #[inline]
    fn is_matching(&self, _query: &Self::Query) -> bool {
        true
    }
}

impl<C: 'static, T: ComponentList, M1: Marker, M2: Marker, N: Entity<T, M2>> Entity<T, Cons<M1, M2>>
    for Cons<Option<GenVecIndex<C>>, N>
where
    T: Contains<GenVec<C>, M1>,
{
    type Query = Cons<Expected<C>, N::Query>;
    type Builder = Cons<Option<CollectionType<C, GenVec<C>>>, N::Builder>;

    #[inline]
    fn is_matching(&self, query: &Self::Query) -> bool {
        if self.head.is_some() && query.is_expected() {
            self.tail.is_matching(&query.tail)
        } else {
            false
        }
    }
}

pub struct SystemListBuilder<T: ComponentList, M: Marker, E: Entity<T, M>, S: SystemList<T, M, E>> {
    systems: S,
    _marker: PhantomData<(T, M, E)>,
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> SystemListBuilder<T, M, E, Nil> {
    pub fn new() -> Self {
        Self {
            systems: Nil::new(),
            _marker: PhantomData,
        }
    }
}

impl<T: ComponentList, M1: Marker, E: Entity<T, M1>, S: SystemList<T, M1, E>>
    SystemListBuilder<T, M1, E, S>
{
    pub fn with_system<M2: Marker, M3: Marker, N: System<EntityComponentContext<T, M1, E>>>(
        self,
        system: N,
    ) -> SystemListBuilder<T, M1, E, Cons<SystemExecutor<T, M1, M2, E, N>, S>>
    where
        N::Components: IntoSubsetIterator<T, M2> + QueryWrite<E::Query, M3>,
    {
        SystemListBuilder {
            systems: Cons::new(SystemExecutor::new(system), self.systems),
            _marker: PhantomData,
        }
    }

    pub fn build(self) -> EntityComponentSystem<T, M1, E, S> {
        EntityComponentSystem {
            storage: EntityComponentContext::default(),
            systems: self.systems,
            _marker: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct Archetype<T: ComponentList, M: Marker, E: Entity<T, M>> {
    index: GenVecIndex<Self>,
    query: E::Query,
    lookup: HashSet<GenVecIndex<E>>,
    indices: GenVec<GenVecIndex<E>>,
    entities: GenVec<E>,
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
            index: GenVecIndex::invalid(),
            query: E::Query::default(),
            entities: GenVec::new(),
            indices: GenVec::new(),
            lookup: HashSet::new(),
            components: T::default(),
            _marker: PhantomData,
        }
    }

    fn set_index(&mut self, index: GenVecIndex<Self>) {
        self.index = index;
    }

    #[inline]
    pub fn from_entity(entity: EntityBuilder<T, M, E>) -> Self {
        let EntityBuilder {
            query_builder,
            entity_builder,
            ..
        } = entity;
        let mut components = T::default();
        let entity = entity_builder.insert(&mut components).unwrap();
        let mut entities = GenVec::new();
        let index = entities.push(entity).unwrap();
        let mut indices = GenVec::new();
        indices.push(index).unwrap();
        let lookup = HashSet::from([index]);
        Self {
            index: GenVecIndex::invalid(),
            query: query_builder.build(),
            entities,
            components,
            indices,
            lookup,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn is_matching(&self, query: &E::Query) -> bool {
        self.query == *query
    }

    #[inline]
    pub fn push_entity(&mut self, entity: EntityBuilder<T, M, E>) {
        let entity = entity.build();
        let entity = entity.insert(&mut self.components).unwrap();
        let index = self.entities.push(entity).unwrap();
        self.indices.push(index).unwrap();
        self.lookup.insert(index);
    }

    #[inline]
    pub fn sub_iter<'a, M2: Marker, N: IntoSubsetIterator<T, M2> + 'a>(
        &'a self,
    ) -> impl Iterator<Item = N::RefList<'a>> {
        ListIter::iter_sub::<_, _, N>(&self.components)
            .all()
            .map(|entity| N::unwrap_ref(entity))
    }

    #[inline]
    pub fn sub_iter_entity<'a, M2: Marker, N: IntoSubsetIterator<T, M2> + 'a>(
        &'a self,
    ) -> impl Iterator<Item = EntityRef<'a, T, M, M2, E, N>> {
        // Entity components and its corresponding entity index are pushed/removed into the collections
        // in the same order, this should result in them being stored at the same index in GenVec internal storage
        // thus is safe to assume that zip will yield the correct pairs
        self.sub_iter::<_, N>()
            .zip((&self.indices).into_iter())
            .map(|(components, &entity)| EntityRef::new(self, entity, components))
    }

    pub fn try_get_entity<'a>(&'a self, index: EntityIndexTyped<T, M, E>) -> Option<E::Ref<'a>> {
        if self.index == index.archetype && self.lookup.contains(&index.entity) {
            let entity = self.entities.get(index.entity).ok()?;
            let components = entity.get_ref(&self.components).ok()?;
            Some(components)
        } else {
            None
        }
    }
}

pub struct EntityBuilder<T: ComponentList, M: Marker, E: Entity<T, M>> {
    query_builder: QueryBuilder<T, M, E>,
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
            query_builder: QueryBuilder::default(),
            entity_builder: E::Builder::default(),
        }
    }

    #[inline]
    pub fn with_component<C: 'static, M2: Marker>(self, component: C) -> Self
    where
        E::Builder: Contains<Option<CollectionType<C, GenVec<C>>>, M2>,
        E::Query: Contains<Expected<C>, M2>,
    {
        let Self {
            mut entity_builder,
            mut query_builder,
        } = self;
        let _ = entity_builder
            .get_mut()
            .insert(CollectionType::new(component));
        query_builder = query_builder.with_expected();
        Self {
            query_builder,
            entity_builder,
        }
    }

    #[inline]
    pub fn build(self) -> E::Builder {
        self.entity_builder
    }
}

pub enum Operation<C: ComponentList, M: Marker, E: Entity<C, M>> {
    Push(EntityBuilder<C, M, E>),
}

pub struct OperationQueue<C: ComponentList, M: Marker, E: Entity<C, M>> {
    operations: Vec<Operation<C, M, E>>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Default for OperationQueue<C, M, E> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> OperationQueue<C, M, E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }

    #[inline]
    pub fn process(self, world: &mut EntityComponentContext<C, M, E>) {
        self.operations
            .into_iter()
            .for_each(|operation| match operation {
                Operation::Push(entity) => world.push_entity(entity),
            });
    }

    #[inline]
    pub fn get_entity_builder(&self) -> EntityBuilder<C, M, E> {
        EntityBuilder::new()
    }

    #[inline]
    pub fn push_entity(&mut self, entity: EntityBuilder<C, M, E>) {
        self.operations.push(Operation::Push(entity));
    }
}

pub trait EntityComponentConfiguration {
    type Components: ComponentList;
    type Marker: Marker;
    type Entity: Entity<Self::Components, Self::Marker>;
    type Context;

    #[inline]
    fn builder() -> SystemListBuilder<Self::Components, Self::Marker, Self::Entity, Nil> {
        SystemListBuilder::new()
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

pub type ContextQueue<C> = OperationQueue<
    <C as EntityComponentConfiguration>::Components,
    <C as EntityComponentConfiguration>::Marker,
    <C as EntityComponentConfiguration>::Entity,
>;

pub struct EntityIndexTyped<C: ComponentList, M: Marker, E: Entity<C, M>> {
    archetype: GenVecIndex<Archetype<C, M, E>>,
    entity: GenVecIndex<E>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Clone for EntityIndexTyped<C, M, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Copy for EntityIndexTyped<C, M, E> {}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Debug for EntityIndexTyped<C, M, E> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityIndexTyped")
            .field("archetype", &self.archetype)
            .field("entity", &self.entity)
            .finish()
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> EntityIndexTyped<C, M, E> {
    pub fn new(archetype: &Archetype<C, M, E>, entity: GenVecIndex<E>) -> Self {
        Self {
            archetype: archetype.index,
            entity,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityIndex {
    archetype: TypeGuard<GenIndexRaw>,
    entity: TypeGuard<GenIndexRaw>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> From<EntityIndexTyped<C, M, E>> for EntityIndex {
    fn from(index: EntityIndexTyped<C, M, E>) -> Self {
        Self {
            archetype: index.archetype.into_guard(),
            entity: index.entity.into_guard(),
        }
    }
}

impl EntityIndex {
    pub fn in_context<C: EntityComponentConfiguration>(
        &self,
    ) -> EntityIndexTyped<C::Components, C::Marker, C::Entity> {
        let archetype = GenVecIndex::try_from_guard(self.archetype).unwrap();
        let entity = GenVecIndex::try_from_guard(self.entity).unwrap();
        EntityIndexTyped { archetype, entity }
    }
}

pub struct EntityRef<
    'a,
    C: ComponentList,
    M1: Marker,
    M2: Marker,
    E: Entity<C, M1>,
    N: IntoSubsetIterator<C, M2> + 'a,
> {
    index: EntityIndexTyped<C, M1, E>,
    components: N::RefList<'a>,
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
        archetype: &Archetype<C, M1, E>,
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

pub struct EntityComponentContext<C: ComponentList, M: Marker, E: Entity<C, M>> {
    archetypes: GenVec<Archetype<C, M, E>>,
    lookup: HashSet<GenVecIndex<Archetype<C, M, E>>>,
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
            lookup: HashSet::new(),
        }
    }

    pub fn push_entity(&mut self, entity: EntityBuilder<C, M, E>) {
        let archetype = self
            .iter_mut()
            .find(|archetype| archetype.is_matching(&entity));
        match archetype {
            Some(archetype) => archetype.push_entity(entity),
            None => {
                let archetype = self
                    .archetypes
                    .push(Archetype::from_entity(entity))
                    .unwrap();
                self.archetypes[archetype].set_index(archetype);
                self.lookup.insert(archetype);
            }
        }
    }

    pub fn iter_ref<'a>(&'a self) -> impl Iterator<Item = &'a Archetype<C, M, E>> {
        (&self.archetypes).into_iter()
    }

    fn iter_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut Archetype<C, M, E>> {
        (&mut self.archetypes).into_iter()
    }

    fn query<'a, M2: Marker, N: IntoSubsetIterator<C, M2> + QueryWrite<E::Query, M2> + 'a>(
        &'a self,
    ) -> impl Iterator<Item = EntityRef<'a, C, M, M2, E, N>> {
        let query = N::write(E::Query::default());
        self.iter_ref()
            .filter(move |archetype| query.is_subset(&archetype.query))
            .flat_map(|archetype| archetype.sub_iter_entity())
    }

    fn try_get_entity<'a>(&'a self, index: EntityIndexTyped<C, M, E>) -> Option<E::Ref<'a>> {
        self.lookup
            .contains(&index.archetype)
            .then_some(self.archetypes[index.archetype].try_get_entity(index))
            .flatten()
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

pub struct EntityComponentSystem<
    C: ComponentList,
    M: Marker,
    E: Entity<C, M>,
    S: SystemList<C, M, E>,
> {
    storage: EntityComponentContext<C, M, E>,
    systems: S,
    _marker: PhantomData<(C, M)>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>, S: SystemList<C, M, E>>
    EntityComponentSystem<C, M, E, S>
{
    #[inline]
    pub fn get_entity_builder(&self) -> EntityBuilder<C, M, E> {
        EntityBuilder::new()
    }

    pub fn push_entity(&mut self, entity: EntityBuilder<C, M, E>) {
        self.storage.push_entity(entity);
    }

    #[inline]
    pub fn execute_systems(&mut self) {
        let mut operation_queue = OperationQueue::new();
        self.storage.iter_ref().for_each(|archetype| {
            self.systems
                .execute(archetype, &self.storage, &mut operation_queue);
        });
        operation_queue.process(&mut self.storage);
    }
}

#[cfg(test)]
mod test_ecs {
    use std::{fmt::Debug, marker::PhantomData};

    use type_kit::{list_type, unpack_list, Cons, GenVec, GenVecIndex, Here, Nil, There, TypeList};

    use crate::ecs::{
        ContextQueue, Entity, EntityComponentConfiguration, EntityComponentContext, EntityIndex,
        System,
    };

    type EscContextType = ecs_context_type![String, u32, u16, Option<EntityIndex>, Nil];

    struct TestSystem<T: 'static + Debug> {
        _marker: PhantomData<T>,
    }

    impl<T: 'static + Debug> TestSystem<T> {
        pub fn new() -> Self {
            Self {
                _marker: PhantomData,
            }
        }
    }

    impl<T: 'static + Debug> System<EscContextType> for TestSystem<T> {
        type Components = list_type![T, Nil];

        fn execute<'a>(
            &self,
            unpack_list![borrowed_value]: <Self::Components as TypeList>::RefList<'a>,
            context: &EscContextType,
            queue: &mut ContextQueue<EscContextType>,
        ) {
            println!(
                "Executing TestSystem<{}> with components: {:?}",
                std::any::type_name::<T>(),
                borrowed_value
            );
            queue.push_entity(
                queue
                    .get_entity_builder()
                    .with_component("GeneratedComponent".to_string()),
            );
        }
    }

    struct TestSystemMulti<T: 'static + Debug, N: 'static + Debug> {
        _marker: PhantomData<(T, N)>,
    }

    impl<T: 'static + Debug, N: 'static + Debug> TestSystemMulti<T, N> {
        pub fn new() -> Self {
            Self {
                _marker: PhantomData,
            }
        }
    }

    impl<T: 'static + Debug, N: 'static + Debug> System<EscContextType> for TestSystemMulti<T, N> {
        type Components = list_type![T, N, Nil];

        fn execute<'a>(
            &self,
            unpack_list![borrowed_first, borrowed_second]: <Self::Components as TypeList>::RefList<
                'a,
            >,
            _context: &EscContextType,
            _queue: &mut ContextQueue<EscContextType>,
        ) {
            println!(
                "Executing TestSystem<{}, {}> with components: {:?}, {:?}",
                std::any::type_name::<T>(),
                std::any::type_name::<N>(),
                borrowed_first,
                borrowed_second
            );
        }
    }

    pub struct TestEntityQuery;

    impl System<EscContextType> for TestEntityQuery {
        type Components = list_type![u16, Nil];

        fn execute<'a>(
            &self,
            unpack_list![_borrow_u16]: <Self::Components as TypeList>::RefList<'a>,
            context: &EscContextType,
            queue: &mut ContextQueue<EscContextType>,
        ) {
            let _ = context
                .query::<_, list_type![String, Nil]>()
                .for_each(|entity_ref| {
                    let index: EntityIndex = entity_ref.index.into();
                    println!(
                        "Executing TestEntityQuery with entity components: {:?}",
                        entity_ref.components
                    );
                    queue.push_entity(queue.get_entity_builder().with_component(Some(index)));
                });
        }
    }

    pub struct TestEntityTryGet;

    impl System<EscContextType> for TestEntityTryGet {
        type Components = list_type![Option<EntityIndex>, Nil];

        fn execute<'a>(
            &self,
            unpack_list![entity_index]: <Self::Components as TypeList>::RefList<'a>,
            context: &EscContextType,
            _queue: &mut ContextQueue<EscContextType>,
        ) {
            if let Some(index) = entity_index {
                if let Some(components) =
                    context.try_get_entity(index.in_context::<EscContextType>())
                {
                    let string_component: &Option<&String> = components.get();
                    if let Some(value) = string_component {
                        println!("TestEntityTryGet found entity with component: {}", value);
                    } else {
                        println!(
                            "TestEntityTryGet found entity with index but no String component",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_ecs() {
        let mut ecs = EscContextType::builder()
            .with_system(TestSystem::<String>::new())
            .with_system(TestSystem::<u32>::new())
            .with_system(TestSystem::<u16>::new())
            .with_system(TestSystemMulti::<String, u32>::new())
            .with_system(TestEntityQuery)
            .with_system(TestEntityTryGet)
            .build();
        let entity = ecs.get_entity_builder().with_component("Hello".to_string());
        ecs.push_entity(entity);
        let entity = ecs.get_entity_builder().with_component("World".to_string());
        ecs.push_entity(entity);
        let entity = ecs
            .get_entity_builder()
            .with_component("The Answer".to_string())
            .with_component(42u32);
        ecs.push_entity(entity);
        let entity = ecs.get_entity_builder().with_component(2u32);
        ecs.push_entity(entity);
        let entity = ecs.get_entity_builder().with_component(1u16);
        ecs.push_entity(entity);
        ecs.execute_systems();

        println!("\n\tECS executed successfully first!\n");

        ecs.execute_systems();

        println!("\n\tECS executed successfully second!\n");

        ecs.execute_systems();

        println!("\n\tECS executed successfully third!\n");
    }
}
