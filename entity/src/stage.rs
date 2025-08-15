use std::marker::PhantomData;

use type_kit::{Cons, IntoSubsetIterator, Marker, Nil, Subset, TypeList};

use crate::{
    context::{ComponentListType, EntityComponentContext, EntityQueryType},
    entity::{Query, QueryWrite},
    operation::OperationChannel,
    system::{self, System, SystemExecutor, SystemList, SystemListBuilder},
    EntityComponentSystem, EntityComponentSystemContext, ExternalSystem,
};

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

pub trait Builder<E: EntityComponentContext, C: ExternalSystem> {
    fn with_system<M2: Marker, M3: Marker, M4: Marker, M5: Marker, N: System<E>>(
        self,
        system: N,
    ) -> impl Builder<E, C>
    where
        N::Components:
            IntoSubsetIterator<ComponentListType<E>, M2> + QueryWrite<EntityQueryType<E>, M3>,
        N::WriteList: QueryWrite<EntityQueryType<E>, M4>,
        N::External: Subset<C, M5>;

    fn barrier(self) -> impl Builder<E, C>;

    fn build(self) -> impl EntityComponentSystem<E, C>;
}

pub struct StageListBuilder<
    E: EntityComponentContext,
    C: ExternalSystem,
    L: system::Builder<E, C>,
    S: StageList<E, C>,
> {
    builder: L,
    stages: S,
    _marker: PhantomData<(E, C)>,
}

impl<E: EntityComponentContext, C: ExternalSystem>
    StageListBuilder<E, C, SystemListBuilder<E, C, Nil>, Nil>
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
        E: EntityComponentContext,
        C: ExternalSystem,
        L: system::Builder<E, C>,
        S: StageList<E, C>,
    > Builder<E, C> for StageListBuilder<E, C, L, S>
{
    fn with_system<M2: Marker, M3: Marker, M4: Marker, M5: Marker, N: System<E>>(
        self,
        system: N,
    ) -> impl Builder<E, C>
    where
        N::Components:
            IntoSubsetIterator<ComponentListType<E>, M2> + QueryWrite<EntityQueryType<E>, M3>,
        N::WriteList: QueryWrite<EntityQueryType<E>, M4>,
        N::External: Subset<C, M5>,
    {
        let system = SystemExecutor::new(system);
        if !system
            .component_write()
            .get_intersection(&system::Builder::component_write(&self.builder))
            .is_empty()
        {
            panic!("New system's write access is a subset of existing systems");
        }
        StageListBuilder {
            builder: system::Builder::with_executor(self.builder, system),
            stages: self.stages,
            _marker: PhantomData,
        }
    }

    fn barrier(self) -> impl Builder<E, C> {
        StageListBuilder {
            builder: SystemListBuilder::new(),
            stages: Cons::new(
                Stage::new(system::Builder::build(self.builder)),
                self.stages,
            ),
            _marker: PhantomData,
        }
    }

    fn build(self) -> impl EntityComponentSystem<E, C> {
        EntityComponentSystemContext {
            context: E::default(),
            stages: Cons::new(
                Stage::new(system::Builder::build(self.builder)),
                self.stages,
            ),
            _marker: PhantomData,
        }
    }
}
