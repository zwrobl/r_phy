use std::marker::PhantomData;

use rayon::Scope;
use type_kit::{Cons, IntoSubsetIterator, Marker, Nil, Subset, TypeList};

use crate::ecs::{
    context::{EntityComponentConfiguration, EntityComponentContext},
    entity::{Entity, Query, QueryWrite},
    index::EntityIndex,
    operation::{ContextQueue, OperationChannel, OperationSender},
    ArchetypeRef, ComponentList, EntityComponentSystem, ExternalSystem,
};

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
    _marker: PhantomData<(T, M, E, C)>,
}

impl<T: ComponentList, C: ExternalSystem, M: Marker, E: Entity<T, M>>
    StageListBuilder<T, C, M, E, Nil, Nil>
{
    pub fn new() -> Self {
        StageListBuilder {
            builder: SystemListBuilder::new(),
            stages: Nil::new(),
            _marker: PhantomData,
        }
    }
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
            _marker: PhantomData,
        }
    }

    pub fn barrier(self) -> StageListBuilder<T, C, M1, E, Nil, Cons<Stage<T, M1, E, C, L>, S>> {
        StageListBuilder {
            builder: SystemListBuilder::new(),
            stages: Cons::new(Stage::new(self.builder.build()), self.stages),
            _marker: PhantomData,
        }
    }

    pub fn build(self) -> EntityComponentSystem<T, C, M1, E, Cons<Stage<T, M1, E, C, L>, S>> {
        EntityComponentSystem {
            storage: EntityComponentContext::default(),
            stages: Cons::new(Stage::new(self.builder.build()), self.stages),
            _marker: PhantomData,
        }
    }
}
