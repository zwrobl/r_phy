use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Formatter},
    hash::Hash,
    hash::Hasher,
    marker::PhantomData,
    ops::Deref,
};

use type_kit::{
    CollectionType, Cons, Contains, FromGuard, GenCollection, GenIndexRaw, GenVec, GenVecIndex,
    IntoCollectionIterator, IntoSubsetIterator, ListIter, MarkedIndexList, MarkedItemList, Marker,
    Nil, OptionalList, TypeGuard, TypeList,
};

pub trait System<T: EntityComponentConfiguration>: 'static {
    type Components: TypeList;

    fn execute<'a>(
        &self,
        entity: EntityIndex,
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
            query: <S::Components as QueryWrite<E::Query, M3>>::write(E::Query::default()),
            system,
            _phantom: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn execute<'a, 'b>(
        &'a self,
        index: GenVecIndex<Archetype<L, M1, E>>,
        archetype: &'b Archetype<L, M1, E>,
        context: &EntityComponentContext<L, M1, E>,
        operation_queue: &mut OperationQueue<L, M1, E>,
    ) {
        if self.is_matching(archetype) {
            archetype
                .sub_iter_entity::<_, S::Components>(index)
                .for_each(|entity| {
                    self.system.execute(
                        entity.index.into(),
                        entity.components,
                        context,
                        operation_queue,
                    );
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
        index: GenVecIndex<Archetype<T, M, E>>,
        archetype: &Archetype<T, M, E>,
        context: &EntityComponentContext<T, M, E>,
        operation_queue: &mut OperationQueue<T, M, E>,
    );
}

impl<T: ComponentList, M: Marker, E: Entity<T, M>> SystemList<T, M, E> for Nil {
    fn execute<'a>(
        &'a self,
        _index: GenVecIndex<Archetype<T, M, E>>,
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
        index: GenVecIndex<Archetype<L, M1, E>>,
        archetype: &Archetype<L, M1, E>,
        context: &EntityComponentContext<L, M1, E>,
        operation_queue: &mut OperationQueue<L, M1, E>,
    ) {
        self.head
            .execute(index, archetype, context, operation_queue);
        self.tail
            .execute(index, archetype, context, operation_queue);
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

pub enum ComponentUpdate<C: 'static> {
    Update(C),
    Remove,
    Keep,
}

impl<C: 'static> Default for ComponentUpdate<C> {
    #[inline]
    fn default() -> Self {
        Self::Keep
    }
}

impl<'a, C: 'static> From<&'a ComponentUpdate<C>> for Expected<C> {
    #[inline]
    fn from(value: &'a ComponentUpdate<C>) -> Self {
        match value {
            ComponentUpdate::Remove => Expected::new(false),
            _ => Expected::new(true),
        }
    }
}

pub trait Entity<C: ComponentList, M: Marker>:
    MarkedIndexList<C, M> + OptionalList + Clone + Copy + 'static
{
    type Query: Default + Clone + Copy + 'static + Query;
    type Builder: MarkedItemList<C, M, IndexList = Self> + OptionalList + Default;
    type Update: Default + 'static;

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

impl<C: 'static, T: ComponentList, M1: Marker, M2: Marker, N: Entity<T, M2>> Entity<T, Cons<M1, M2>>
    for Cons<Option<GenVecIndex<C>>, N>
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
    query: E::Query,
    lookup: HashMap<GenVecIndex<E>, GenVecIndex<GenVecIndex<E>>>,
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
            query: E::Query::default(),
            entities: GenVec::new(),
            indices: GenVec::new(),
            lookup: HashMap::new(),
            components: T::default(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn try_set_archetype(
        &mut self,
        index: GenVecIndex<Self>,
        entity: EntityBuilder<T, M, E>,
    ) -> Option<EntityIndexTyped<T, M, E>> {
        if self.entities.is_empty() {
            self.query = entity.query_builder;
            Some(self.push_entity(index, entity))
        } else {
            None
        }
    }

    #[inline]
    pub fn is_matching(&self, query: &E::Query) -> bool {
        self.query == *query
    }

    #[inline]
    pub fn push_entity(
        &mut self,
        archetype_index: GenVecIndex<Archetype<T, M, E>>,
        entity: EntityBuilder<T, M, E>,
    ) -> EntityIndexTyped<T, M, E> {
        let entity = entity.build();
        let entity = entity.insert(&mut self.components).unwrap();
        let index = self.entities.push(entity).unwrap();
        let mapping = self.indices.push(index).unwrap();
        self.lookup.insert(index, mapping);
        EntityIndexTyped::new(archetype_index, index)
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
        index: GenVecIndex<Self>,
    ) -> impl Iterator<Item = EntityRef<'a, T, M, M2, E, N>> {
        // Entity components and its corresponding entity index are pushed/removed into the collections
        // in the same order, this should result in them being stored at the same index in GenVec internal storage
        // thus is safe to assume that zip will yield the correct pairs
        self.sub_iter::<_, N>()
            .zip((&self.indices).into_iter())
            .map(move |(components, &entity)| EntityRef::new(index, entity, components))
    }

    pub fn try_pop_entity<'a>(&'a mut self, index: EntityIndexTyped<T, M, E>) -> Option<E::Owned> {
        if self.lookup.contains_key(&index.entity) {
            let entity = self.entities.pop(index.entity).ok()?;
            let components = entity.get_owned(&mut self.components).ok()?;
            self.indices.pop(self.lookup[&index.entity]).ok()?;
            self.lookup.remove(&index.entity);
            Some(components)
        } else {
            None
        }
    }

    pub fn try_get_entity<'a>(&'a self, index: EntityIndexTyped<T, M, E>) -> Option<E::Ref<'a>> {
        if self.lookup.contains_key(&index.entity) {
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
        if self.lookup.contains_key(&index.entity) {
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
    pub fn with_component<C: 'static, M2: Marker>(self, component: C) -> Self
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

pub struct EntityUpdate<C: ComponentList, M: Marker, E: Entity<C, M>> {
    index: EntityIndexTyped<C, M, E>,
    components: E::Update,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> EntityUpdate<C, M, E> {
    #[inline]
    pub fn new(index: EntityIndexTyped<C, M, E>) -> Self {
        Self {
            index,
            components: E::Update::default(),
        }
    }

    #[inline]
    pub fn update<C2: 'static, M2: Marker>(mut self, component: C2) -> Self
    where
        E::Update: Contains<ComponentUpdate<C2>, M2>,
    {
        *self.components.get_mut() = ComponentUpdate::Update(component);
        self
    }

    #[inline]
    pub fn remove<C2: 'static, M2: Marker>(mut self) -> Self
    where
        E::Update: Contains<ComponentUpdate<C2>, M2>,
    {
        *self.components.get_mut() = ComponentUpdate::Remove;
        self
    }
}

pub enum UpdateResult<C: ComponentList, M: Marker, E: Entity<C, M>> {
    ArchetypeChanged((EntityBuilder<C, M, E>, PersistentIndexTyped<C, M, E>)),
    NotFound(EntityUpdate<C, M, E>),
    InPlace,
}

pub enum Operation<C: ComponentList, M: Marker, E: Entity<C, M>> {
    Push(EntityBuilder<C, M, E>),
    Pop(EntityIndexTyped<C, M, E>),
    Update(EntityUpdate<C, M, E>),
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
        let mut updated = HashMap::new();
        let mut removed = HashSet::new();
        self.operations
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

    #[inline]
    pub fn push_entity(&mut self, entity: EntityBuilder<C, M, E>) {
        self.operations.push(Operation::Push(entity));
    }

    #[inline]
    pub fn pop_entity(&mut self, entity: EntityIndexTyped<C, M, E>) {
        self.operations.push(Operation::Pop(entity));
    }

    #[inline]
    pub fn update_entity(&mut self, entity: EntityUpdate<C, M, E>) {
        self.operations.push(Operation::Update(entity));
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

pub struct PersistentIndexTyped<C: ComponentList, M: Marker, E: Entity<C, M>> {
    index: GenVecIndex<EntityIndexTyped<C, M, E>>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Hash for PersistentIndexTyped<C, M, E> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> PartialEq for PersistentIndexTyped<C, M, E> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Eq for PersistentIndexTyped<C, M, E> {}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Clone for PersistentIndexTyped<C, M, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Copy for PersistentIndexTyped<C, M, E> {}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> Debug for PersistentIndexTyped<C, M, E> {
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

impl<C: ComponentList, M: Marker, E: Entity<C, M>> From<PersistentIndexTyped<C, M, E>>
    for PersistentIndex
{
    #[inline]
    fn from(index: PersistentIndexTyped<C, M, E>) -> Self {
        Self {
            index: index.index.into_guard(),
        }
    }
}

impl PersistentIndex {
    #[inline]
    pub fn in_context<C: EntityComponentConfiguration>(
        &self,
    ) -> PersistentIndexTyped<C::Components, C::Marker, C::Entity> {
        let index = GenVecIndex::try_from_guard(self.index).unwrap();
        PersistentIndexTyped { index }
    }
}

pub struct PersistentIndexMap<C: ComponentList, M: Marker, E: Entity<C, M>> {
    lookup: HashMap<EntityIndexTyped<C, M, E>, GenVecIndex<EntityIndexTyped<C, M, E>>>,
    entities: GenVec<EntityIndexTyped<C, M, E>>,
}

impl<C: ComponentList, M: Marker, E: Entity<C, M>> PersistentIndexMap<C, M, E> {
    #[inline]
    pub fn new() -> Self {
        Self {
            lookup: HashMap::new(),
            entities: GenVec::new(),
        }
    }

    #[inline]
    pub fn register(&mut self, entity: EntityIndexTyped<C, M, E>) {
        if !self.lookup.contains_key(&entity) {
            let index_mapping = self.entities.push(entity).unwrap();
            self.lookup.insert(entity, index_mapping);
        }
    }

    #[inline]
    pub fn unregister(&mut self, entity: EntityIndexTyped<C, M, E>) {
        if let Some(index_mapping) = self.lookup.remove(&entity) {
            self.entities.pop(index_mapping).unwrap();
        }
    }

    #[inline]
    pub fn update(
        &mut self,
        index: PersistentIndexTyped<C, M, E>,
        entity: EntityIndexTyped<C, M, E>,
    ) {
        let PersistentIndexTyped { index } = index;
        if let Ok(registered) = self.entities.get(index) {
            if *registered != entity {
                self.entities[index] = entity;
                self.lookup.remove(&entity);
                self.lookup.insert(entity, index);
            }
        }
    }

    #[inline]
    pub fn get_index(&self, entity: EntityIndexTyped<C, M, E>) -> PersistentIndexTyped<C, M, E> {
        let index = *self.lookup.get(&entity).unwrap();
        PersistentIndexTyped { index }
    }

    #[inline]
    pub fn try_get_entity(
        &self,
        index: PersistentIndexTyped<C, M, E>,
    ) -> Option<EntityIndexTyped<C, M, E>> {
        let PersistentIndexTyped { index } = index;
        self.entities.get(index).ok().copied()
    }
}

pub struct EntityComponentContext<C: ComponentList, M: Marker, E: Entity<C, M>> {
    archetypes: GenVec<Archetype<C, M, E>>,
    indices: GenVec<GenVecIndex<Archetype<C, M, E>>>,
    lookup: HashMap<GenVecIndex<Archetype<C, M, E>>, GenVecIndex<GenVecIndex<Archetype<C, M, E>>>>,
    persistent_map: PersistentIndexMap<C, M, E>,
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
            indices: GenVec::new(),
            lookup: HashMap::new(),
            persistent_map: PersistentIndexMap::new(),
        }
    }

    pub fn push_entity(
        &mut self,
        entity: EntityBuilder<C, M, E>,
        persistent_index: Option<PersistentIndexTyped<C, M, E>>,
    ) {
        let archetype = self
            .iter_mut()
            .find(|(archetype, _)| archetype.is_matching(&entity));
        let entity = match archetype {
            Some((archetype, index)) => archetype.push_entity(*index, entity),
            None => {
                let archetype = self.archetypes.push(Archetype::new()).unwrap();
                let index_mapping = self.indices.push(archetype).unwrap();
                self.lookup.insert(archetype, index_mapping);
                self.archetypes[archetype]
                    .try_set_archetype(archetype, entity)
                    .unwrap()
            }
        };
        if let Some(persistent_index) = persistent_index {
            self.persistent_map.update(persistent_index, entity);
        } else {
            self.persistent_map.register(entity);
        }
    }

    pub fn pop_entity(&mut self, index: EntityIndexTyped<C, M, E>) -> Option<E::Owned> {
        let removed = self
            .lookup
            .contains_key(&index.archetype)
            .then_some(self.archetypes[index.archetype].try_pop_entity(index))
            .flatten();
        if removed.is_some() {
            self.persistent_map.unregister(index);
        }
        removed
    }

    pub fn update_entity(&mut self, update: EntityUpdate<C, M, E>) -> UpdateResult<C, M, E> {
        if self.lookup.contains_key(&update.index.archetype) {
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
                    let persistent_index = self.persistent_map.get_index(update.index);
                    return UpdateResult::ArchetypeChanged((builder, persistent_index));
                }
            }
        }
        UpdateResult::NotFound(update)
    }

    pub fn iter_ref<'a>(
        &'a self,
    ) -> impl Iterator<Item = (&'a Archetype<C, M, E>, &'a GenVecIndex<Archetype<C, M, E>>)> {
        (&self.archetypes)
            .into_iter()
            .zip((&self.indices).into_iter())
    }

    fn iter_mut<'a>(
        &'a mut self,
    ) -> impl Iterator<
        Item = (
            &'a mut Archetype<C, M, E>,
            &'a GenVecIndex<Archetype<C, M, E>>,
        ),
    > {
        (&mut self.archetypes)
            .into_iter()
            .zip((&self.indices).into_iter())
    }

    pub fn query<'a, M2: Marker, N: IntoSubsetIterator<C, M2> + QueryWrite<E::Query, M2> + 'a>(
        &'a self,
    ) -> impl Iterator<Item = EntityRef<'a, C, M, M2, E, N>> {
        let query = N::write(E::Query::default());
        self.iter_ref()
            .filter(move |(archetype, ..)| query.is_subset(&archetype.query))
            .flat_map(|(archetype, &index)| archetype.sub_iter_entity(index))
    }

    pub fn try_get_entity<'a>(&'a self, index: EntityIndexTyped<C, M, E>) -> Option<E::Ref<'a>> {
        self.lookup
            .contains_key(&index.archetype)
            .then_some(self.archetypes[index.archetype].try_get_entity(index))
            .flatten()
    }

    pub fn get_persistent_index(
        &self,
        entity: EntityIndexTyped<C, M, E>,
    ) -> PersistentIndexTyped<C, M, E> {
        self.persistent_map.get_index(entity)
    }

    pub fn try_map_persistent(
        &self,
        index: PersistentIndexTyped<C, M, E>,
    ) -> Option<EntityIndexTyped<C, M, E>> {
        self.persistent_map.try_get_entity(index)
    }

    pub fn get_entity_builder(&self) -> EntityBuilder<C, M, E> {
        EntityBuilder::new()
    }

    pub fn get_entity_update_builder(
        &self,
        index: EntityIndexTyped<C, M, E>,
    ) -> EntityUpdate<C, M, E> {
        EntityUpdate::new(index)
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
        self.storage.push_entity(entity, None);
    }

    #[inline]
    pub fn execute_systems(&mut self) {
        let mut operation_queue = OperationQueue::new();
        self.storage.iter_ref().for_each(|(archetype, &index)| {
            self.systems
                .execute(index, archetype, &self.storage, &mut operation_queue);
        });
        operation_queue.process(&mut self.storage);
    }
}

#[cfg(test)]
mod test_ecs {
    use std::{fmt::Debug, marker::PhantomData};

    use type_kit::{list_type, unpack_list, Cons, GenVec, GenVecIndex, Here, Nil, There, TypeList};

    use crate::ecs::{
        ContextQueue, EntityComponentConfiguration, EntityComponentContext, EntityIndex,
        PersistentIndex, System,
    };

    type EscContextType = ecs_context_type![
        String,
        u32,
        u16,
        Option<EntityIndex>,
        Option<PersistentIndex>,
        Nil
    ];

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
            _entity: EntityIndex,
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
                context
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
            _entity: EntityIndex,
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
            entity: EntityIndex,
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
                    queue.push_entity(context.get_entity_builder().with_component(Some(index)));
                });
            queue.update_entity(
                context
                    .get_entity_update_builder(entity.in_context::<EscContextType>())
                    .update("UpdatedQueryEntity".to_string())
                    .remove::<u16, _>(),
            );
        }
    }

    pub struct TestEntityTryGet;

    impl System<EscContextType> for TestEntityTryGet {
        type Components = list_type![Option<EntityIndex>, Nil];

        fn execute<'a>(
            &self,
            entity: EntityIndex,
            unpack_list![entity_index]: <Self::Components as TypeList>::RefList<'a>,
            context: &EscContextType,
            queue: &mut ContextQueue<EscContextType>,
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
        type Components = list_type![Option<PersistentIndex>, Nil];

        fn execute<'a>(
            &self,
            entity: EntityIndex,
            unpack_list![persistent_index]: <Self::Components as TypeList>::RefList<'a>,
            context: &EscContextType,
            queue: &mut ContextQueue<EscContextType>,
        ) {
            if persistent_index.is_none() {
                let persistent: Option<PersistentIndex> = context
                    .query::<_, list_type![u16, Nil]>()
                    .next()
                    .map(|entity_ref| context.get_persistent_index(entity_ref.index).into());
                queue.update_entity(
                    context
                        .get_entity_update_builder(entity.in_context::<EscContextType>())
                        .update(persistent),
                );
                println!(
                    "TestEntityPersistentIndex starts tracking new persistent index: {:?}",
                    persistent
                );
            } else {
                let index = context
                    .try_map_persistent(persistent_index.unwrap().in_context::<EscContextType>());
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
    fn test_ecs() {
        let mut ecs = EscContextType::builder()
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
}
