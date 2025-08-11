use std::{marker::PhantomData, ops::Deref};

use type_kit::{
    CollectionType, Cons, Contains, GenIndex, GenVec, MarkedBorrowList, MarkedIndexList,
    MarkedItemList, Marker, Nil, Subset, TypeList,
};

pub trait System<T: EntityComponentConfiguration>: 'static {
    type Query: TypeList;
    type Components: TypeList;

    fn execute<'a>(
        &'a mut self,
        components: <Self::Components as TypeList>::RefList<'a>,
        queue: &mut ContextQueue<T>,
    );
}

pub trait ComponentList: TypeList + Default + 'static {}

impl ComponentList for Nil {}

impl<C: 'static, N: ComponentList> ComponentList for Cons<GenVec<C>, N> {}

pub struct SystemExecutor<
    L: ComponentList,
    M1: Marker,
    M2: Marker,
    M3: Marker,
    E: Entity<L, M1>,
    C: Subset<E::Borrowed, M2>,
    S: System<EntityComponentContext<L, M1, E>, Components = C>,
> where
    S::Query: QueryWrite<E::Query, M3>,
{
    query: E::Query,
    system: S,
    _phantom: std::marker::PhantomData<(L, M1, M2, M3, C)>,
}

impl<
        L: ComponentList,
        M1: Marker,
        M2: Marker,
        M3: Marker,
        E: Entity<L, M1>,
        C: Subset<E::Borrowed, M2>,
        S: System<EntityComponentContext<L, M1, E>, Components = C>,
    > SystemExecutor<L, M1, M2, M3, E, C, S>
where
    S::Query: QueryWrite<E::Query, M3>,
{
    #[inline]
    pub fn new(system: S) -> Self {
        Self {
            query: S::Query::write(E::Query::default()),
            system,
            _phantom: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn execute<'a>(
        &'a mut self,
        archetype: &mut Archetype<L, M1, E>,
        operation_queue: &mut OperationQueue<L, M1, E>,
    ) {
        if self.query.is_subset(&archetype.query) {
            archetype.execute_system(&mut self.system, operation_queue);
        }
    }

    #[inline]
    pub fn is_matching(&self, archetype: &E::Query) -> bool {
        self.query.is_subset(archetype)
    }
}

pub trait SystemList<T: ComponentList, M: Marker, E: Entity<T, M>> {
    fn execute<'a>(
        &'a mut self,
        archetype: &mut Archetype<T, M, E>,
        operation_queue: &mut OperationQueue<T, M, E>,
    );
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> SystemList<T, M, E> for Nil {
    fn execute<'a>(
        &'a mut self,
        _archetype: &mut Archetype<T, M, E>,
        _operation_queue: &mut OperationQueue<T, M, E>,
    ) {
    }
}

impl<
        L: ComponentList,
        M1: Marker,
        M2: Marker,
        M3: Marker,
        E: Entity<L, M1>,
        C: Subset<E::Borrowed, M2>,
        S: System<EntityComponentContext<L, M1, E>, Components = C>,
        N: SystemList<L, M1, E>,
    > SystemList<L, M1, E> for Cons<SystemExecutor<L, M1, M2, M3, E, C, S>, N>
where
    S::Query: QueryWrite<E::Query, M3>,
{
    fn execute<'a>(
        &'a mut self,
        archetype: &mut Archetype<L, M1, E>,
        operation_queue: &mut OperationQueue<L, M1, E>,
    ) {
        self.head.execute(archetype, operation_queue);
        self.tail.execute(archetype, operation_queue);
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
    for Cons<Option<GenIndex<C, GenVec<C>>>, N>
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

impl<T: ComponentList, M: Marker, E: Entity<T, M>, S: SystemList<T, M, E>>
    SystemListBuilder<T, M, E, S>
{
    pub fn with_system<
        M2: Marker,
        M3: Marker,
        C: Subset<E::Borrowed, M2>,
        N: System<EntityComponentContext<T, M, E>, Components = C>,
    >(
        self,
        system: N,
    ) -> SystemListBuilder<T, M, E, Cons<SystemExecutor<T, M, M2, M3, E, C, N>, S>>
    where
        N::Query: QueryWrite<E::Query, M3>,
    {
        SystemListBuilder {
            systems: Cons::new(SystemExecutor::new(system), self.systems),
            _marker: PhantomData,
        }
    }

    pub fn build(self) -> EntityComponentSystem<T, M, E, S> {
        EntityComponentSystem {
            storage: EntityComponentContext::default(),
            systems: self.systems,
            _marker: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct Archetype<T: ComponentList, M: Marker, E: Entity<T, M>> {
    query: E::Query,
    // TODO: Cange to GenVec when it impl Iterator
    entities: Vec<E>,
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
            entities: Vec::new(),
            components: T::default(),
            _marker: PhantomData,
        }
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
        let entities = vec![entity];
        Self {
            query: query_builder.build(),
            entities,
            components,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn is_matching(&self, entity: &EntityBuilder<T, M, E>) -> bool {
        self.query == **entity
    }

    #[inline]
    pub fn push_entity(&mut self, entity: EntityBuilder<T, M, E>) {
        let entity = entity.build();
        let entity = entity.insert(&mut self.components).unwrap();
        self.entities.push(entity);
    }

    pub fn execute_system<
        M2: Marker,
        C: Subset<E::Borrowed, M2>,
        S: System<EntityComponentContext<T, M, E>, Components = C>,
    >(
        &mut self,
        system: &mut S,
        operation_queue: &mut OperationQueue<T, M, E>,
    ) {
        self.entities.iter().for_each(|&entity| {
            let components = entity.get_borrowed(&mut self.components).unwrap();
            system.execute(C::sub_get(&components), operation_queue);
            components.put_back(&mut self.components).unwrap();
        });
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
}

pub type ContextQueue<C> = OperationQueue<
    <C as EntityComponentConfiguration>::Components,
    <C as EntityComponentConfiguration>::Marker,
    <C as EntityComponentConfiguration>::Entity,
>;

pub struct EntityComponentContext<C: ComponentList, M: Marker, E: Entity<C, M>> {
    // TODO: This should be changed to GenVec when its support iteration
    // to allow for safe inter-archetype entity references
    archetypes: Vec<Archetype<C, M, E>>,
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
            archetypes: Vec::new(),
        }
    }

    pub fn push_entity(&mut self, entity: EntityBuilder<C, M, E>) {
        let archetype = self
            .archetypes
            .iter_mut()
            .find(|archetype| archetype.is_matching(&entity));
        match archetype {
            Some(archetype) => archetype.push_entity(entity),
            None => {
                let archetype = Archetype::from_entity(entity);
                self.archetypes.push(archetype);
            }
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
    [$component:ty, $last:ty] => { Cons<Option<GenIndex<$component, GenVec<$component>>>, $last> };
    [$component:ty $(, $components:ty)*] => {
        Cons<Option<GenIndex<$component, GenVec<$component>>>, entity_type![$($components),*]>
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
        self.storage.archetypes.iter_mut().for_each(|archetype| {
            self.systems.execute(archetype, &mut operation_queue);
        });
        operation_queue.process(&mut self.storage);
    }
}

#[cfg(test)]
mod test_ecs {
    use std::{fmt::Debug, marker::PhantomData};

    use type_kit::{
        list_type, unpack_list, Borrowed, Cons, GenIndex, GenVec, Here, Nil, There, TypeList,
    };

    use crate::ecs::{ContextQueue, EntityComponentConfiguration, EntityComponentContext, System};

    type EscContextType = ecs_context_type![String, u32, u16, Nil];

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
        type Query = list_type![T, Nil];
        type Components = list_type![Option<Borrowed<T, GenVec<T>>>, Nil];

        fn execute<'a>(
            &'a mut self,
            unpack_list![borrowed_value]: <Self::Components as TypeList>::RefList<'a>,
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
        type Query = list_type![T, N, Nil];
        type Components = list_type![
            Option<Borrowed<T, GenVec<T>>>,
            Option<Borrowed<N, GenVec<N>>>,
            Nil
        ];

        fn execute<'a>(
            &'a mut self,
            unpack_list![borrowed_first, borrowed_second]: <Self::Components as TypeList>::RefList<
                'a,
            >,
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

    #[test]
    fn test_ecs() {
        let mut ecs = EscContextType::builder()
            .with_system(TestSystem::<String>::new())
            .with_system(TestSystem::<u32>::new())
            .with_system(TestSystem::<u16>::new())
            .with_system(TestSystemMulti::<String, u32>::new())
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

        println!("ECS executed successfully first!");

        ecs.execute_systems();
    }
}
