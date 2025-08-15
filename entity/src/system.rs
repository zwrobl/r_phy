use std::marker::PhantomData;

use rayon::Scope;
use type_kit::{Cons, IntoSubsetIterator, Marker, Nil, Subset, TypeList};

use crate::{
    context::{ComponentListType, EntityComponentContext, EntityQueryType},
    entity::{Query, QueryWrite},
    index::EntityIndex,
    operation::{OperationChannel, OperationSender},
    ArchetypeRef, EntityComponentSystemContext, ExternalSystem,
};

pub trait System<E: EntityComponentContext>: Sync {
    type External: TypeList;
    type WriteList: TypeList;
    type Components: TypeList;

    fn execute<'a>(
        &self,
        entity: EntityIndex,
        components: <Self::Components as TypeList>::RefList<'a>,
        context: &E,
        queue: &OperationSender<E>,
        external: <Self::External as TypeList>::RefList<'a>,
    );
}

pub struct SystemExecutor<
    E: EntityComponentContext,
    M2: Marker,
    M3: Marker,
    C: TypeList,
    S: System<E>,
> where
    S::Components: IntoSubsetIterator<E::Components, M2>,
    S::External: Subset<C, M3>,
{
    query: EntityQueryType<E>,
    write: EntityQueryType<E>,
    system: S,
    _phantom: std::marker::PhantomData<(C, M2, M3)>,
}

impl<E: EntityComponentContext, M2: Marker, M3: Marker, C: TypeList, S: System<E>>
    SystemExecutor<E, M2, M3, C, S>
where
    S::Components: IntoSubsetIterator<E::Components, M2>,
    S::External: Subset<C, M3>,
{
    #[inline]
    pub fn new<M4: Marker, M5: Marker>(system: S) -> Self
    where
        S::Components: QueryWrite<EntityQueryType<E>, M4>,
        S::WriteList: QueryWrite<EntityQueryType<E>, M5>,
    {
        Self {
            query: <S::Components as QueryWrite<EntityQueryType<E>, M4>>::write(
                EntityQueryType::<E>::default(),
            ),
            write: <S::WriteList as QueryWrite<EntityQueryType<E>, M5>>::write(
                EntityQueryType::<E>::default(),
            ),
            system,
            _phantom: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn execute<'a, 'b>(
        &'a self,
        archetype: ArchetypeRef<'b, E>,
        context: &E,
        operation_queue: &OperationSender<E>,
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
    pub fn is_matching(&self, archetype: ArchetypeRef<'_, E>) -> bool {
        self.query.is_subset(&archetype.query)
    }

    #[inline]
    pub fn component_write(&self) -> EntityQueryType<E> {
        self.write
    }
}

pub trait SystemList<E: EntityComponentContext, C: TypeList>: Sync {
    fn execute<'a, 'b>(
        &'a self,
        _scope: &'b Scope<'a>,
        context: &'a E,
        operation_queue: OperationSender<E>,
        external: &'a C,
    ) where
        'a: 'b;

    fn component_write(&self) -> EntityQueryType<E>;
}

impl<E: EntityComponentContext, C: TypeList> SystemList<E, C> for Nil {
    fn execute<'a, 'b>(
        &'a self,
        _scope: &'b Scope<'a>,
        _context: &'a E,
        _operation_queue: OperationSender<E>,
        _external: &'a C,
    ) where
        'a: 'b,
    {
    }

    fn component_write(&self) -> EntityQueryType<E> {
        EntityQueryType::<E>::default()
    }
}

impl<
        E: EntityComponentContext,
        M3: Marker,
        M2: Marker,
        C: ExternalSystem,
        S: System<E>,
        N: SystemList<E, C>,
    > SystemList<E, C> for Cons<SystemExecutor<E, M2, M3, C, S>, N>
where
    S::Components: IntoSubsetIterator<ComponentListType<E>, M2>,
    S::External: Subset<C, M3>,
{
    fn execute<'a, 'b>(
        &'a self,
        scope: &'b Scope<'a>,
        context: &'a E,
        operation_queue: OperationSender<E>,
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

    fn component_write(&self) -> EntityQueryType<E> {
        let head = self.head.component_write();
        let tail = self.tail.component_write();
        head.get_union(&tail)
    }
}

pub trait StageList<E: EntityComponentContext, C: TypeList> {
    type SystemList: SystemList<E, C>;

    fn execute<'a>(&self, context: &mut E, external: &C);

    fn component_write(&self) -> EntityQueryType<E>;
}

impl<E: EntityComponentContext, C: TypeList> StageList<E, C> for Nil {
    type SystemList = Nil;

    #[inline]
    fn execute<'a>(&self, _context: &mut E, _external: &C) {}

    #[inline]
    fn component_write(&self) -> EntityQueryType<E> {
        EntityQueryType::<E>::default()
    }
}

pub struct Stage<E: EntityComponentContext, C: ExternalSystem, L: SystemList<E, C>> {
    systems: L,
    _phantom: PhantomData<(E, C)>,
}

impl<E: EntityComponentContext, C: ExternalSystem, L: SystemList<E, C>> Stage<E, C, L> {
    #[inline]
    pub fn new(systems: L) -> Self {
        Self {
            systems,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn execute<'a>(&self, context: &mut E, external: &C) {
        let (sender, receiver) = OperationChannel::new();
        rayon::scope(|scope| {
            self.systems.execute(scope, &context, sender, external);
        });
        receiver.process(context);
    }

    #[inline]
    pub fn component_write(&self) -> EntityQueryType<E> {
        self.systems.component_write()
    }
}

impl<E: EntityComponentContext, C: ExternalSystem, L: SystemList<E, C>, N: StageList<E, C>>
    StageList<E, C> for Cons<Stage<E, C, L>, N>
{
    type SystemList = L;

    #[inline]
    fn execute<'a>(&self, context: &mut E, external: &C) {
        self.head.execute(context, external);
        self.tail.execute(context, external);
    }

    #[inline]
    fn component_write(&self) -> EntityQueryType<E> {
        self.head.component_write()
    }
}

pub struct SystemListBuilder<E: EntityComponentContext, C: ExternalSystem, S: SystemList<E, C>> {
    systems: S,
    _marker: PhantomData<(E, C)>,
}

impl<E: EntityComponentContext, C: ExternalSystem> SystemListBuilder<E, C, Nil> {
    pub fn new() -> Self {
        SystemListBuilder {
            systems: Nil::new(),
            _marker: PhantomData,
        }
    }
}

impl<E: EntityComponentContext, C: ExternalSystem, S: SystemList<E, C>> SystemListBuilder<E, C, S> {
    pub fn with_system<M2: Marker, M3: Marker, M4: Marker, M5: Marker, N: System<E>>(
        self,
        system: SystemExecutor<E, M2, M5, C, N>,
    ) -> SystemListBuilder<E, C, Cons<SystemExecutor<E, M2, M5, C, N>, S>>
    where
        N::Components:
            IntoSubsetIterator<ComponentListType<E>, M2> + QueryWrite<EntityQueryType<E>, M3>,
        N::WriteList: QueryWrite<EntityQueryType<E>, M4>,
        N::External: Subset<C, M5>,
    {
        SystemListBuilder {
            systems: Cons::new(system, self.systems),
            _marker: PhantomData,
        }
    }

    pub fn component_write(&self) -> EntityQueryType<E> {
        self.systems.component_write()
    }

    pub fn build(self) -> S {
        self.systems
    }
}

pub struct StageListBuilder<
    E: EntityComponentContext,
    C: ExternalSystem,
    L: SystemList<E, C>,
    S: StageList<E, C>,
> {
    builder: SystemListBuilder<E, C, L>,
    stages: S,
    _marker: PhantomData<(E, C)>,
}

impl<E: EntityComponentContext, C: ExternalSystem> StageListBuilder<E, C, Nil, Nil> {
    pub fn new() -> Self {
        StageListBuilder {
            builder: SystemListBuilder::new(),
            stages: Nil::new(),
            _marker: PhantomData,
        }
    }
}

impl<E: EntityComponentContext, C: ExternalSystem, L: SystemList<E, C>, S: StageList<E, C>>
    StageListBuilder<E, C, L, S>
{
    pub fn with_system<M2: Marker, M3: Marker, M4: Marker, M5: Marker, N: System<E>>(
        self,
        system: N,
    ) -> StageListBuilder<E, C, Cons<SystemExecutor<E, M2, M5, C, N>, L>, S>
    where
        N::Components:
            IntoSubsetIterator<ComponentListType<E>, M2> + QueryWrite<EntityQueryType<E>, M3>,
        N::WriteList: QueryWrite<EntityQueryType<E>, M4>,
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

    pub fn barrier(self) -> StageListBuilder<E, C, Nil, Cons<Stage<E, C, L>, S>> {
        StageListBuilder {
            builder: SystemListBuilder::new(),
            stages: Cons::new(Stage::new(self.builder.build()), self.stages),
            _marker: PhantomData,
        }
    }

    pub fn build(self) -> EntityComponentSystemContext<E, C, Cons<Stage<E, C, L>, S>> {
        EntityComponentSystemContext {
            context: E::default(),
            stages: Cons::new(Stage::new(self.builder.build()), self.stages),
            _marker: PhantomData,
        }
    }
}
