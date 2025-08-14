use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::mpsc::{channel, Receiver, Sender},
};

use rayon::Scope;
use type_kit::{
    CollectionType, Cons, Contains, FromGuard, GenCollection, GenIndexRaw, GenVec, GenVecIndex,
    IntoCollectionIterator, IntoSubsetIterator, ListIter, MarkedIndexList, MarkedItemList, Marker,
    Nil, OptionalList, StaticTypeList, Subset, TypeGuard, TypeList,
};

pub trait ExternalSystem: StaticTypeList + Sync {}

impl<T: StaticTypeList + Sync> ExternalSystem for T {}

pub trait ComponentData: Send + Sync + 'static {}

impl<T: Send + Sync + 'static> ComponentData for T {}

pub trait System<T: EntityComponentConfiguration>: Sync {
    type External: TypeList;
    type WriteList: TypeList;
    type Components: TypeList;

    fn execute<'a>(
        &self,
        entity: EntityIndex,
        components: <Self::Components as TypeList>::RefList<'a>,
        context: &T::Context,
        queue: &ContextQueue<T>,
        external: <Self::External as TypeList>::RefList<'a>,
    );
}

pub trait ComponentList: IntoCollectionIterator + Send + Sync {}

impl ComponentList for Nil {}

impl<C: ComponentData, N: ComponentList> ComponentList for Cons<GenVec<C>, N> {}

pub struct SystemExecutor<
    L: ComponentList,
    C: TypeList,
    M1: Marker,
    M2: Marker,
    M3: Marker,
    E: Entity<L, M1>,
    S: System<EntityComponentContext<L, M1, E>>,
> where
    S::Components: IntoSubsetIterator<L, M2>,
    S::External: Subset<C, M3>,
{
    query: E::Query,
    write: E::Query,
    system: S,
    _phantom: std::marker::PhantomData<(L, C, M1, M2, M3)>,
}

impl<
        L: ComponentList,
        C: TypeList,
        M1: Marker,
        M2: Marker,
        M3: Marker,
        E: Entity<L, M1>,
        S: System<EntityComponentContext<L, M1, E>>,
    > SystemExecutor<L, C, M1, M2, M3, E, S>
where
    S::Components: IntoSubsetIterator<L, M2>,
    S::External: Subset<C, M3>,
{
    #[inline]
    pub fn new<M4: Marker, M5: Marker>(system: S) -> Self
    where
        S::Components: QueryWrite<E::Query, M4>,
        S::WriteList: QueryWrite<E::Query, M5>,
    {
        Self {
            query: <S::Components as QueryWrite<E::Query, M4>>::write(E::Query::default()),
            write: <S::WriteList as QueryWrite<E::Query, M5>>::write(E::Query::default()),
            system,
            _phantom: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn execute<'a, 'b>(
        &'a self,
        archetype: ArchetypeRef<'b, L, M1, E>,
        context: &EntityComponentContext<L, M1, E>,
        operation_queue: &OperationSender<L, M1, E>,
        external: &C,
    ) {
        if self.is_matching(archetype) {
            archetype
                .sub_iter_entity::<_, S::Components>()
                .for_each(|entity| {
                    self.system.execute(
                        entity.index.into(),
                        entity.components,
                        context,
                        operation_queue,
                        S::External::sub_get(external),
                    );
                });
        }
    }

    #[inline]
    pub fn is_matching(&self, archetype: ArchetypeRef<'_, L, M1, E>) -> bool {
        self.query.is_subset(&archetype.query)
    }

    #[inline]
    pub fn component_write(&self) -> E::Query {
        self.write
    }
}

pub trait SystemList<T: ComponentList, M: Marker, E: Entity<T, M>, C: TypeList>: Sync {
    fn execute<'a, 'b>(
        &'a self,
        _scope: &'b Scope<'a>,
        context: &'a EntityComponentContext<T, M, E>,
        operation_queue: OperationSender<T, M, E>,
        external: &'a C,
    ) where
        'a: 'b;

    fn component_write(&self) -> E::Query;
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>, C: TypeList> SystemList<T, M, E, C> for Nil {
    fn execute<'a, 'b>(
        &'a self,
        _scope: &'b Scope<'a>,
        _context: &'a EntityComponentContext<T, M, E>,
        _operation_queue: OperationSender<T, M, E>,
        _external: &'a C,
    ) where
        'a: 'b,
    {
    }

    fn component_write(&self) -> E::Query {
        E::Query::default()
    }
}

impl<
        L: ComponentList,
        C: ExternalSystem,
        M1: Marker,
        M2: Marker,
        M3: Marker,
        E: Entity<L, M1>,
        S: System<EntityComponentContext<L, M1, E>>,
        N: SystemList<L, M1, E, C>,
    > SystemList<L, M1, E, C> for Cons<SystemExecutor<L, C, M1, M2, M3, E, S>, N>
where
    S::Components: IntoSubsetIterator<L, M2>,
    S::External: Subset<C, M3>,
{
    fn execute<'a, 'b>(
        &'a self,
        scope: &'b Scope<'a>,
        context: &'a EntityComponentContext<L, M1, E>,
        operation_queue: OperationSender<L, M1, E>,
        external: &'a C,
    ) where
        'a: 'b,
    {
        {
            let operation_queue = operation_queue.clone();
            scope.spawn(move |_| {
                context.iter_ref().for_each(|archetype| {
                    self.head
                        .execute(archetype, context, &operation_queue, external);
                })
            });
        }
        self.tail.execute(scope, context, operation_queue, external);
    }

    fn component_write(&self) -> E::Query {
        let head = self.head.component_write();
        let tail = self.tail.component_write();
        head.get_union(&tail)
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

pub trait StageList<T: ComponentList, M: Marker, E: Entity<T, M>, C: TypeList> {
    type SystemList: SystemList<T, M, E, C>;

    fn execute<'a>(&self, context: &mut EntityComponentContext<T, M, E>, external: &C);

    fn component_write(&self) -> E::Query;
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>, C: TypeList> StageList<T, M, E, C> for Nil {
    type SystemList = Nil;

    #[inline]
    fn execute<'a>(&self, _context: &mut EntityComponentContext<T, M, E>, _external: &C) {}

    #[inline]
    fn component_write(&self) -> E::Query {
        E::Query::default()
    }
}

pub struct Stage<
    T: ComponentList,
    M: Marker,
    E: Entity<T, M>,
    C: ExternalSystem,
    L: SystemList<T, M, E, C>,
> {
    systems: L,
    _phantom: PhantomData<(T, M, E, C)>,
}

impl<
        T: ComponentList,
        M: Marker,
        E: Entity<T, M>,
        C: ExternalSystem,
        L: SystemList<T, M, E, C>,
    > Stage<T, M, E, C, L>
{
    #[inline]
    pub fn new(systems: L) -> Self {
        Self {
            systems,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn execute<'a>(&self, context: &mut EntityComponentContext<T, M, E>, external: &C) {
        let (sender, receiver) = OperationChannel::new();
        rayon::scope(|scope| {
            self.systems.execute(scope, &context, sender, external);
        });
        receiver.process(context);
    }

    #[inline]
    pub fn component_write(&self) -> E::Query {
        self.systems.component_write()
    }
}

impl<
        T: ComponentList,
        C: ExternalSystem,
        M: Marker,
        E: Entity<T, M>,
        L: SystemList<T, M, E, C>,
        N: StageList<T, M, E, C>,
    > StageList<T, M, E, C> for Cons<Stage<T, M, E, C, L>, N>
{
    type SystemList = L;

    #[inline]
    fn execute<'a>(&self, context: &mut EntityComponentContext<T, M, E>, external: &C) {
        self.head.execute(context, external);
        self.tail.execute(context, external);
    }

    #[inline]
    fn component_write(&self) -> E::Query {
        self.head.component_write()
    }
}

pub struct SystemListBuilder<
    T: ComponentList,
    M: Marker,
    E: Entity<T, M>,
    C: ExternalSystem,
    S: SystemList<T, M, E, C>,
> {
    systems: S,
    _marker: PhantomData<(T, M, E, C)>,
}

impl<T: ComponentList, M1: Marker, E: Entity<T, M1>, C: ExternalSystem>
    SystemListBuilder<T, M1, E, C, Nil>
{
    pub fn new() -> Self {
        SystemListBuilder {
            systems: Nil::new(),
            _marker: PhantomData,
        }
    }
}

impl<
        T: ComponentList,
        M1: Marker,
        E: Entity<T, M1>,
        C: ExternalSystem,
        S: SystemList<T, M1, E, C>,
    > SystemListBuilder<T, M1, E, C, S>
{
    pub fn with_system<
        M2: Marker,
        M3: Marker,
        M4: Marker,
        M5: Marker,
        N: System<EntityComponentContext<T, M1, E>>,
    >(
        self,
        system: SystemExecutor<T, C, M1, M2, M5, E, N>,
    ) -> SystemListBuilder<T, M1, E, C, Cons<SystemExecutor<T, C, M1, M2, M5, E, N>, S>>
    where
        N::Components: IntoSubsetIterator<T, M2> + QueryWrite<E::Query, M3>,
        N::WriteList: QueryWrite<E::Query, M4>,
        N::External: Subset<C, M5>,
    {
        SystemListBuilder {
            systems: Cons::new(system, self.systems),
            _marker: PhantomData,
        }
    }

    pub fn component_write(&self) -> E::Query {
        self.systems.component_write()
    }

    pub fn build(self) -> S {
        self.systems
    }
}

pub struct ExternalListBuilder<T: ComponentList, C: TypeList, M: Marker, E: Entity<T, M>> {
    external: C,
    _marker: PhantomData<(T, M, E)>,
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> ExternalListBuilder<T, Nil, M, E> {
    #[inline]
    pub fn new() -> Self {
        ExternalListBuilder {
            external: Nil::new(),
            _marker: PhantomData,
        }
    }
}

impl<T: ComponentList, C: ExternalSystem, M: Marker, E: Entity<T, M>>
    ExternalListBuilder<T, C, M, E>
{
    pub fn with_external<N>(self, external: N) -> ExternalListBuilder<T, Cons<N, C>, M, E> {
        ExternalListBuilder {
            external: Cons::new(external, self.external),
            _marker: PhantomData,
        }
    }

    pub fn build(self) -> StageListBuilder<T, C, M, E, Nil, Nil> {
        StageListBuilder {
            builder: SystemListBuilder::new(),
            stages: Nil::new(),
            external: self.external,
            _marker: PhantomData,
        }
    }
}

pub struct StageListBuilder<
    T: ComponentList,
    C: ExternalSystem,
    M: Marker,
    E: Entity<T, M>,
    L: SystemList<T, M, E, C>,
    S: StageList<T, M, E, C>,
> {
    builder: SystemListBuilder<T, M, E, C, L>,
    stages: S,
    external: C,
    _marker: PhantomData<(T, M, E)>,
}

impl<
        T: ComponentList,
        C: ExternalSystem,
        M1: Marker,
        E: Entity<T, M1>,
        L: SystemList<T, M1, E, C>,
        S: StageList<T, M1, E, C>,
    > StageListBuilder<T, C, M1, E, L, S>
{
    pub fn with_system<
        M2: Marker,
        M3: Marker,
        M4: Marker,
        M5: Marker,
        N: System<EntityComponentContext<T, M1, E>>,
    >(
        self,
        system: N,
    ) -> StageListBuilder<T, C, M1, E, Cons<SystemExecutor<T, C, M1, M2, M5, E, N>, L>, S>
    where
        N::Components: IntoSubsetIterator<T, M2> + QueryWrite<E::Query, M3>,
        N::WriteList: QueryWrite<E::Query, M4>,
        N::External: Subset<C, M5>,
    {
        let system = SystemExecutor::new(system);
        if !system
            .component_write()
            .get_intersection(&self.builder.component_write())
            .is_empty()
        {
            panic!("New system's write access is a subset of existing systems");
        }
        StageListBuilder {
            builder: self.builder.with_system(system),
            stages: self.stages,
            external: self.external,
            _marker: PhantomData,
        }
    }

    pub fn barrier(self) -> StageListBuilder<T, C, M1, E, Nil, Cons<Stage<T, M1, E, C, L>, S>> {
        StageListBuilder {
            builder: SystemListBuilder::new(),
            stages: Cons::new(Stage::new(self.builder.build()), self.stages),
            external: self.external,
            _marker: PhantomData,
        }
    }

    pub fn build(self) -> EntityComponentSystem<T, C, M1, E, Cons<Stage<T, M1, E, C, L>, S>> {
        EntityComponentSystem {
            storage: EntityComponentContext::default(),
            stages: Cons::new(Stage::new(self.builder.build()), self.stages),
            external: self.external,
            _marker: PhantomData,
        }
    }
}

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
    query: E::Query,
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

pub struct EntityBuilder<T: ComponentList, M: Marker, E: Entity<T, M>> {
    query_builder: E::Query,
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
    index: EntityIndexTyped<C, M, E>,
    components: E::Update,
}

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

pub enum Operation<C: ComponentList, M: Marker, E: Entity<C, M>> {
    Push(EntityBuilder<C, M, E>),
    Pop(EntityIndexTyped<C, M, E>),
    Update(EntityUpdate<C, M, E>),
}

pub struct OperationSender<C: ComponentList, M: Marker, E: Entity<C, M>> {
    sender: Sender<Operation<C, M, E>>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Clone for OperationSender<C, M, E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> OperationSender<C, M, E> {
    #[inline]
    pub fn push_entity(&self, entity: EntityBuilder<C, M, E>) {
        self.sender.send(Operation::Push(entity)).unwrap();
    }

    #[inline]
    pub fn pop_entity(&self, entity: EntityIndexTyped<C, M, E>) {
        self.sender.send(Operation::Pop(entity)).unwrap();
    }

    #[inline]
    pub fn update_entity<W: TypeList>(&self, entity: EntityUpdateBuilder<C, M, E, W>) {
        self.sender.send(Operation::Update(entity.build())).unwrap();
    }
}

pub struct OperationReceiver<C: ComponentList, M: Marker, E: Entity<C, M>> {
    receiver: Receiver<Operation<C, M, E>>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> OperationReceiver<C, M, E> {
    pub fn process(self, world: &mut EntityComponentContext<C, M, E>) {
        let operations: Vec<_> = self.receiver.into_iter().collect();

        let mut updated = HashMap::with_capacity(operations.len());
        let mut removed = HashSet::with_capacity(operations.len());
        operations
            .into_iter()
            .for_each(|operation| match operation {
                Operation::Push(entity) => world.push_entity(entity, None),

                Operation::Pop(index) => {
                    if world.pop_entity(index).is_some() {
                        removed.insert(index);
                    } else if updated.contains_key(&index) {
                        updated.remove(&index);
                        removed.insert(index);
                    }
                }
                Operation::Update(update) => {
                    let index = update.index;
                    if !removed.contains(&index) {
                        match world.update_entity(update) {
                            UpdateResult::ArchetypeChanged(builder) => {
                                updated.insert(index, builder);
                            }
                            UpdateResult::NotFound(update) => {
                                if let Some((builder, ..)) = updated.get_mut(&index) {
                                    builder.update_components(update.components);
                                }
                            }
                            _ => (),
                        }
                    }
                }
            });
        updated
            .into_iter()
            .for_each(|(_, (builder, persistent_index))| {
                world.push_entity(builder, Some(persistent_index));
            });
    }
}

pub struct OperationChannel {}

impl OperationChannel {
    pub fn new<C: ComponentList, M: Marker, E: Entity<C, M>>(
    ) -> (OperationSender<C, M, E>, OperationReceiver<C, M, E>) {
        let (sender, receiver) = channel();
        (OperationSender { sender }, OperationReceiver { receiver })
    }
}

pub trait EntityComponentConfiguration {
    type Components: ComponentList;
    type Marker: Marker;
    type Entity: Entity<Self::Components, Self::Marker>;
    type Context;

    #[inline]
    fn builder() -> ExternalListBuilder<Self::Components, Nil, Self::Marker, Self::Entity> {
        ExternalListBuilder::new()
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

pub type ContextQueue<C> = OperationSender<
    <C as EntityComponentConfiguration>::Components,
    <C as EntityComponentConfiguration>::Marker,
    <C as EntityComponentConfiguration>::Entity,
>;

pub struct EntityIndexTyped<C: ComponentList, M: Marker, E: Entity<C, M>> {
    archetype: GenVecIndex<Archetype<C, M, E>>,
    entity: GenVecIndex<E>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Hash for EntityIndexTyped<C, M, E> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.archetype.hash(state);
        self.entity.hash(state);
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> PartialEq for EntityIndexTyped<C, M, E> {
    fn eq(&self, other: &Self) -> bool {
        self.archetype == other.archetype && self.entity == other.entity
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Eq for EntityIndexTyped<C, M, E> {}

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
    pub fn new(archetype: GenVecIndex<Archetype<C, M, E>>, entity: GenVecIndex<E>) -> Self {
        Self { archetype, entity }
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
    pub fn entity_index<C: EntityComponentConfiguration>(
        &self,
    ) -> PersistentIndexTyped<EntityIndexTyped<C::Components, C::Marker, C::Entity>> {
        let index = GenVecIndex::try_from_guard(self.index).unwrap();
        PersistentIndexTyped { index }
    }
}

#[derive(Debug)]
pub struct PersistentIndexMap<T: Clone + Copy + Eq + Hash + 'static> {
    lookup: HashMap<T, GenVecIndex<T>>,
    items: GenVec<T>,
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
        if let Ok(&registered) = self.items.get(index) {
            if registered != entity {
                self.items[index] = entity;
                self.lookup.remove(&registered);
                self.lookup.insert(entity, index);
            }
        }
    }

    #[inline]
    pub fn into_iter<'a>(&'a self) -> impl Iterator<Item = &'a T> {
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

pub struct EntityComponentSystem<
    T: ComponentList,
    C: TypeList,
    M: Marker,
    E: Entity<T, M>,
    S: StageList<T, M, E, C>,
> {
    storage: EntityComponentContext<T, M, E>,
    stages: S,
    external: C,
    _marker: PhantomData<(T, M)>,
}

impl<T: ComponentList, C: TypeList, M: Marker, E: Entity<T, M>, S: StageList<T, M, E, C>>
    EntityComponentSystem<T, C, M, E, S>
{
    #[inline]
    pub fn get_entity_builder(&self) -> EntityBuilder<T, M, E> {
        EntityBuilder::new()
    }

    pub fn push_entity(&mut self, entity: EntityBuilder<T, M, E>) {
        self.storage.push_entity(entity, None);
    }

    #[inline]
    pub fn execute_systems(&mut self) {
        self.stages.execute(&mut self.storage, &self.external);
    }

    #[inline]
    pub fn get_external(&self) -> &C {
        &self.external
    }

    #[inline]
    pub fn get_external_mut(&mut self) -> &mut C {
        &mut self.external
    }
}

#[cfg(test)]
mod test_ecs {
    use std::{
        fmt::Debug,
        marker::PhantomData,
        sync::{Arc, Mutex},
    };

    use type_kit::{list_type, unpack_list, Cons, GenVec, GenVecIndex, Here, Nil, There, TypeList};

    use crate::ecs::{
        ComponentData, ContextQueue, EntityComponentConfiguration, EntityComponentContext,
        EntityIndex, PersistentIndex, System,
    };

    type EscContextType = ecs_context_type![
        String,
        u32,
        u16,
        Option<EntityIndex>,
        Option<PersistentIndex>,
        Nil
    ];

    struct TestSystem<T: ComponentData> {
        _marker: PhantomData<T>,
    }

    impl<T: ComponentData> TestSystem<T> {
        pub fn new() -> Self {
            Self {
                _marker: PhantomData,
            }
        }
    }

    impl<T: ComponentData + Debug> System<EscContextType> for TestSystem<T> {
        type External = Nil;
        type WriteList = Nil;
        type Components = list_type![T, Nil];

        fn execute<'a>(
            &self,
            _entity: EntityIndex,
            unpack_list![borrowed_value]: <Self::Components as TypeList>::RefList<'a>,
            context: &EscContextType,
            queue: &ContextQueue<EscContextType>,
            _external: <Self::External as TypeList>::RefList<'a>,
        ) {
            println!(
                "Executing TestSystem<{}> with components: {:?}",
                std::any::type_name::<T>(),
                borrowed_value
            );
            queue.push_entity(
                context
                    .get_entity_builder()
                    .with_component("GeneratedComponent".to_string()),
            );
        }
    }

    struct TestSystemMulti<T: ComponentData, N: ComponentData> {
        _marker: PhantomData<(T, N)>,
    }

    impl<T: ComponentData, N: ComponentData> TestSystemMulti<T, N> {
        pub fn new() -> Self {
            Self {
                _marker: PhantomData,
            }
        }
    }

    impl<T: ComponentData + Debug, N: ComponentData + Debug> System<EscContextType>
        for TestSystemMulti<T, N>
    {
        type External = Nil;
        type WriteList = Nil;
        type Components = list_type![T, N, Nil];

        fn execute<'a>(
            &self,
            _entity: EntityIndex,
            unpack_list![borrowed_first, borrowed_second]: <Self::Components as TypeList>::RefList<
                'a,
            >,
            _context: &EscContextType,
            _queue: &ContextQueue<EscContextType>,
            _external: <Self::External as TypeList>::RefList<'a>,
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
        type External = Nil;
        type WriteList = list_type![u16, String, Nil];
        type Components = list_type![u16, Nil];

        fn execute<'a>(
            &self,
            entity: EntityIndex,
            unpack_list![_borrow_u16]: <Self::Components as TypeList>::RefList<'a>,
            context: &EscContextType,
            queue: &ContextQueue<EscContextType>,
            _external: <Self::External as TypeList>::RefList<'a>,
        ) {
            let _ = context
                .query::<_, list_type![String, Nil]>()
                .for_each(|entity_ref| {
                    let index: EntityIndex = entity_ref.index.into();
                    println!(
                        "Executing TestEntityQuery with entity components: {:?}",
                        entity_ref.components
                    );
                    queue.push_entity(context.get_entity_builder().with_component(Some(index)));
                });
            queue.update_entity(
                context
                    .get_entity_update_builder(self, entity.in_context::<EscContextType>())
                    .update("UpdatedQueryEntity".to_string())
                    .remove::<u16, _, _>(),
            );
        }
    }

    pub struct TestEntityTryGet;

    impl System<EscContextType> for TestEntityTryGet {
        type External = Nil;
        type WriteList = Nil;
        type Components = list_type![Option<EntityIndex>, Nil];

        fn execute<'a>(
            &self,
            entity: EntityIndex,
            unpack_list![entity_index]: <Self::Components as TypeList>::RefList<'a>,
            context: &EscContextType,
            queue: &ContextQueue<EscContextType>,
            _external: <Self::External as TypeList>::RefList<'a>,
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
                queue.pop_entity(entity.in_context::<EscContextType>());
            }
        }
    }

    pub struct TestEntityPersistentIndex;

    impl System<EscContextType> for TestEntityPersistentIndex {
        type External = Nil;
        type WriteList = list_type![Option<PersistentIndex>, Nil];
        type Components = list_type![Option<PersistentIndex>, Nil];

        fn execute<'a>(
            &self,
            entity: EntityIndex,
            unpack_list![persistent_index]: <Self::Components as TypeList>::RefList<'a>,
            context: &EscContextType,
            queue: &ContextQueue<EscContextType>,
            _external: <Self::External as TypeList>::RefList<'a>,
        ) {
            if persistent_index.is_none() {
                let persistent: Option<PersistentIndex> = context
                    .query::<_, list_type![u16, Nil]>()
                    .next()
                    .map(|entity_ref| context.get_persistent_index(entity_ref.index).into());
                queue.update_entity(
                    context
                        .get_entity_update_builder(self, entity.in_context::<EscContextType>())
                        .update(persistent),
                );
                println!(
                    "TestEntityPersistentIndex starts tracking new persistent index: {:?}",
                    persistent
                );
            } else {
                let index = context
                    .try_map_persistent(persistent_index.unwrap().entity_index::<EscContextType>());
                if let Some(index) = index {
                    let entity = context.try_get_entity(index).unwrap();
                    println!(
                        "TestEntityPersistentIndex tracks entity with persistent index: {:?}",
                        entity
                    );
                } else {
                    println!("TestEntityPersistentIndex could not find entity with given persistent index");
                }
            }
        }
    }

    #[test]
    fn test_ecs_execution() {
        let mut ecs = EscContextType::builder()
            .build()
            .with_system(TestSystem::<String>::new())
            .with_system(TestSystem::<u32>::new())
            .with_system(TestSystem::<u16>::new())
            .with_system(TestSystemMulti::<String, u32>::new())
            .with_system(TestEntityQuery)
            .with_system(TestEntityTryGet)
            .with_system(TestEntityPersistentIndex)
            .build();
        let entity = ecs.get_entity_builder().with_component("Hello".to_string());
        ecs.push_entity(entity);
        let entity = ecs
            .get_entity_builder()
            .with_component("World".to_string())
            .with_component::<Option<PersistentIndex>, _>(None);
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

    #[test]
    #[should_panic(expected = "New system's write access is a subset of existing systems")]
    fn test_ecs_stage_write_conflict() {
        let _ = EscContextType::builder()
            .build()
            .with_system(TestEntityQuery)
            .with_system(TestEntityQuery)
            .build();
    }

    #[test]
    fn test_ecs_stage_write_conflict_barier() {
        let _ = EscContextType::builder()
            .build()
            .with_system(TestEntityQuery)
            .barrier()
            .with_system(TestEntityQuery)
            .build();
    }

    #[test]
    fn test_component_update_on_barrier() {
        let mut ecs = EscContextType::builder()
            .build()
            .with_system(TestEntityPersistentIndex)
            .barrier()
            .with_system(TestEntityPersistentIndex)
            .build();

        let entity = ecs.get_entity_builder().with_component(1u16);
        ecs.push_entity(entity);
        let entity = ecs
            .get_entity_builder()
            .with_component::<Option<PersistentIndex>, _>(None);
        ecs.push_entity(entity);
        ecs.execute_systems();
    }

    pub struct ExternalSystem {
        messages: Arc<Mutex<Vec<String>>>,
    }

    impl ExternalSystem {
        pub fn new() -> Self {
            Self {
                messages: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    pub struct TestExternalSystemAcces;

    impl System<EscContextType> for TestExternalSystemAcces {
        type External = list_type![ExternalSystem, Nil];
        type WriteList = Nil;
        type Components = list_type![String, Nil];

        fn execute<'a>(
            &self,
            _entity: EntityIndex,
            unpack_list![component]: <Self::Components as TypeList>::RefList<'a>,
            _context: &<EscContextType as EntityComponentConfiguration>::Context,
            _queue: &ContextQueue<EscContextType>,
            unpack_list![external]: <Self::External as TypeList>::RefList<'a>,
        ) {
            println!("TestExternalSystemAcces received component: {}", component);
            external
                .messages
                .lock()
                .unwrap()
                .push(format!("ExternalSystem received component: {}", component));
        }
    }

    #[test]
    fn test_external_system_access() {
        let mut ecs = EscContextType::builder()
            .with_external(ExternalSystem::new())
            .build()
            .with_system(TestExternalSystemAcces)
            .build();

        let entity = ecs.get_entity_builder().with_component("Hello".to_string());
        ecs.push_entity(entity);

        let entity = ecs
            .get_entity_builder()
            .with_component("World".to_string())
            .with_component(1u32);
        ecs.push_entity(entity);

        let entity = ecs
            .get_entity_builder()
            .with_component("TheAnswer".to_string())
            .with_component(2u16);
        ecs.push_entity(entity);

        ecs.execute_systems();

        let external_system = ecs.get_external().get::<ExternalSystem, _>();
        let messages = external_system.messages.lock().unwrap();
        assert_eq!(
            messages.len(),
            3,
            "External system should have received three messages"
        );
        assert_eq!(messages[0], "ExternalSystem received component: Hello");
        assert_eq!(messages[1], "ExternalSystem received component: World");
        assert_eq!(messages[2], "ExternalSystem received component: TheAnswer");
    }
}
