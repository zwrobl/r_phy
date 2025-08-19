use std::marker::PhantomData;

use type_kit::{Cons, IntoSubsetIterator, Marker, Nil, Subset};

use crate::{
    EntityComponentSystem, EntityComponentSystemContext, ExternalSystem,
    context::{ComponentListType, EntityComponentContext, EntityQueryType},
    entity::{ComponentQuery, UpdateMapWriter},
    operation::{OperationChannel, OperationQueue},
    system::{
        self, GlobalSystem, GlobalSystemExecutor, System, SystemExecutor, SystemList,
        SystemListBuilder,
    },
};

pub trait StageList<E: EntityComponentContext, C: ExternalSystem> {
    type SystemList: SystemList<E, C>;

    fn execute(&self, context: &mut E, external: &C);

    fn component_write(&self) -> EntityQueryType<E>;
}

impl<E: EntityComponentContext, C: ExternalSystem> StageList<E, C> for Nil {
    type SystemList = Nil;

    #[inline]
    fn execute(&self, _context: &mut E, _external: &C) {}

    #[inline]
    fn component_write(&self) -> EntityQueryType<E> {
        EntityQueryType::<E>::default()
    }
}

pub trait Strategy<E: EntityComponentContext, C: ExternalSystem> {
    fn execute<T: SystemList<E, C>>(
        context: &E,
        external: &C,
        queue: OperationChannel<'_, E>,
        systems: &T,
    );
}

pub struct Parallel;

impl<E: EntityComponentContext, C: ExternalSystem> Strategy<E, C> for Parallel {
    fn execute<T: SystemList<E, C>>(
        context: &E,
        external: &C,
        queue: OperationChannel<'_, E>,
        systems: &T,
    ) {
        rayon::scope(|scope| {
            let executor = system::Parallel::new(scope);
            systems.execute(executor, context, queue, external);
        });
    }
}

pub struct Synchronous;

impl<E: EntityComponentContext, C: ExternalSystem> Strategy<E, C> for Synchronous {
    fn execute<T: SystemList<E, C>>(
        context: &E,
        external: &C,
        queue: OperationChannel<'_, E>,
        systems: &T,
    ) {
        let executor = system::Synchronous;
        systems.execute(executor, context, queue, external);
    }
}

pub struct Stage<
    E: EntityComponentContext,
    C: ExternalSystem,
    L: SystemList<E, C>,
    S: Strategy<E, C>,
> {
    systems: L,
    _phantom: PhantomData<(E, C, S)>,
}

impl<E: EntityComponentContext, C: ExternalSystem, L: SystemList<E, C>, S: Strategy<E, C>>
    Stage<E, C, L, S>
{
    #[inline]
    pub fn new(systems: L) -> Self {
        Self {
            systems,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn execute(&self, context: &mut E, external: &C) {
        let mut queue = OperationQueue::new();
        S::execute(context, external, queue.take_channel(), &self.systems);
        queue.process(context);
    }

    #[inline]
    pub fn component_write(&self) -> EntityQueryType<E> {
        self.systems.component_write()
    }
}

impl<
    E: EntityComponentContext,
    C: ExternalSystem,
    L: SystemList<E, C>,
    N: StageList<E, C>,
    S: Strategy<E, C>,
> StageList<E, C> for Cons<Stage<E, C, L, S>, N>
{
    type SystemList = L;

    #[inline]
    fn execute(&self, context: &mut E, external: &C) {
        self.tail.execute(context, external);
        self.head.execute(context, external);
    }

    #[inline]
    fn component_write(&self) -> EntityQueryType<E> {
        self.head.component_write()
    }
}

pub trait Builder<E: EntityComponentContext, C: ExternalSystem> {
    fn with_system<M1: Marker, M2: Marker, M3: Marker, N: System<E>>(
        self,
        system: N,
    ) -> impl Builder<E, C>
    where
        N::Components:
            IntoSubsetIterator<ComponentListType<E>, M1> + ComponentQuery<ComponentListType<E>, M1>,
        N::WriteList: ComponentQuery<ComponentListType<E>, M2>,
        N::External: Subset<C, M3>;

    fn with_global_system<M1: Marker, M2: Marker, N: GlobalSystem<E>>(
        self,
        system: N,
    ) -> impl Builder<E, C>
    where
        N::WriteList: ComponentQuery<ComponentListType<E>, M1>,
        N::External: Subset<C, M2>;

    fn next_stage<T: Strategy<E, C>>(self) -> impl Builder<E, C>;

    fn build<M: Marker>(self) -> impl EntityComponentSystem<E, C>
    where
        ComponentListType<E>: UpdateMapWriter<E, M>;
}

pub struct StageListBuilder<
    E: EntityComponentContext,
    C: ExternalSystem,
    T: Strategy<E, C>,
    L: system::Builder<E, C>,
    S: StageList<E, C>,
> {
    builder: L,
    stages: S,
    _marker: PhantomData<(E, C, T)>,
}

impl<E: EntityComponentContext, C: ExternalSystem, T: Strategy<E, C>> Default
    for StageListBuilder<E, C, T, SystemListBuilder<E, C, Nil>, Nil>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E: EntityComponentContext, C: ExternalSystem, T: Strategy<E, C>>
    StageListBuilder<E, C, T, SystemListBuilder<E, C, Nil>, Nil>
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
    T: Strategy<E, C>,
    L: system::Builder<E, C>,
    S: StageList<E, C>,
> Builder<E, C> for StageListBuilder<E, C, T, L, S>
{
    fn with_system<M1: Marker, M2: Marker, M3: Marker, N: System<E>>(
        self,
        system: N,
    ) -> impl Builder<E, C>
    where
        N::Components:
            IntoSubsetIterator<ComponentListType<E>, M1> + ComponentQuery<ComponentListType<E>, M1>,
        N::WriteList: ComponentQuery<ComponentListType<E>, M2>,
        N::External: Subset<C, M3>,
    {
        let system = SystemExecutor::new(system);
        if !system
            .component_write()
            .get_intersection(system::Builder::component_write(&self.builder))
            .is_empty()
        {
            panic!("New system's write access is a subset of existing systems");
        }
        StageListBuilder {
            builder: system::Builder::with_executor(self.builder, system),
            stages: self.stages,
            _marker: PhantomData::<(E, C, T)>,
        }
    }

    fn with_global_system<M1: Marker, M2: Marker, N: GlobalSystem<E>>(
        self,
        system: N,
    ) -> impl Builder<E, C>
    where
        N::WriteList: ComponentQuery<ComponentListType<E>, M1>,
        N::External: Subset<C, M2>,
    {
        let system = GlobalSystemExecutor::new(system);
        if !system
            .component_write()
            .get_intersection(system::Builder::component_write(&self.builder))
            .is_empty()
        {
            panic!("New system's write access is a subset of existing systems");
        }
        StageListBuilder {
            builder: system::Builder::with_global_executor(self.builder, system),
            stages: self.stages,
            _marker: PhantomData::<(E, C, T)>,
        }
    }

    fn next_stage<N: Strategy<E, C>>(self) -> impl Builder<E, C> {
        StageListBuilder {
            builder: SystemListBuilder::new(),
            stages: Cons::new(
                Stage::<_, _, _, T>::new(system::Builder::build(self.builder)),
                self.stages,
            ),
            _marker: PhantomData::<(E, C, N)>,
        }
    }

    fn build<M2: Marker>(self) -> impl EntityComponentSystem<E, C>
    where
        ComponentListType<E>: UpdateMapWriter<E, M2>,
    {
        let mut context = E::default();
        context.write_update_map::<M2>();
        EntityComponentSystemContext {
            context,
            stages: Cons::new(
                Stage::<_, _, _, T>::new(system::Builder::build(self.builder)),
                self.stages,
            ),
            _marker: PhantomData,
        }
    }
}
