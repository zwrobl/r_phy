use std::marker::PhantomData;

use rayon::Scope;
use type_kit::{
    Cons, IntoSubsetIterator, Marker, Nil, NonEmptyList, RefList, Subset, TypeList, UContains,
};

use crate::{
    ArchetypeRef, ComponentData, ExternalSystem,
    context::{ComponentListType, EntityComponentContext, EntityQueryType, EntityUpdateType},
    entity::{ComponentQuery, ComponentUpdate, EntityUpdate},
    index::{EntityIndex, EntityIndexTyped},
    operation::OperationChannel,
};

pub trait System<E: EntityComponentContext>: Sync {
    type External: TypeList;
    type WriteList: TypeList;
    type Components: NonEmptyList;

    fn execute(
        &self,
        entity: EntityIndex,
        components: RefList<'_, Self::Components>,
        context: &E,
        queue: &OperationChannel<'_, E>,
        external: RefList<'_, Self::External>,
    );

    fn get_entity_update<C: ComponentData, M: Marker>(
        &self,
        index: EntityIndexTyped<E>,
        component: ComponentUpdate<C>,
    ) -> EntityUpdate<E>
    where
        EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
    {
        EntityUpdate::new(index, component)
    }
}

pub trait Executor<'a, E: EntityComponentContext, C: ExternalSystem>:
    Copy + Clone + Send + Sync
{
    fn execute<F: Fn() + Send + 'a>(&self, executor: F);
}

#[derive(Debug, Clone, Copy)]
pub struct Parallel<'a, 'b> {
    scope: &'b Scope<'a>,
}

impl<'a, 'b> Parallel<'a, 'b> {
    pub fn new(scope: &'b Scope<'a>) -> Self {
        Self { scope }
    }
}

impl<'a, 'b, E: EntityComponentContext, C: ExternalSystem> Executor<'a, E, C> for Parallel<'a, 'b> {
    fn execute<F: Fn() + Send + 'a>(&self, executor: F) {
        self.scope.spawn(move |_| {
            (executor)();
        });
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Synchronous;

impl<'a, E: EntityComponentContext, C: ExternalSystem> Executor<'a, E, C> for Synchronous {
    fn execute<F: Fn() + Send + 'a>(&self, executor: F) {
        (executor)();
    }
}

pub struct SystemExecutor<
    E: EntityComponentContext,
    M2: Marker,
    M3: Marker,
    C: ExternalSystem,
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

impl<E: EntityComponentContext, M2: Marker, M3: Marker, C: ExternalSystem, S: System<E>>
    SystemExecutor<E, M2, M3, C, S>
where
    S::Components: IntoSubsetIterator<E::Components, M2>,
    S::External: Subset<C, M3>,
{
    #[inline]
    pub fn new<M4: Marker, M5: Marker>(system: S) -> Self
    where
        S::Components: ComponentQuery<ComponentListType<E>, M4>,
        S::WriteList: ComponentQuery<ComponentListType<E>, M5>,
    {
        Self {
            query: <S::Components as ComponentQuery<_, _>>::query(),
            write: <S::WriteList as ComponentQuery<_, _>>::query(),
            system,
            _phantom: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn execute<'a, 'b>(
        &'a self,
        archetype: ArchetypeRef<'b, E>,
        context: &E,
        operation_queue: &OperationChannel<'_, E>,
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

pub trait SystemList<E: EntityComponentContext, C: ExternalSystem>: Sync {
    fn execute<'a, T: Executor<'a, E, C>>(
        &'a self,
        excutor: T,
        context: &'a E,
        operation_queue: OperationChannel<'a, E>,
        external: &'a C,
    );

    fn component_write(&self) -> EntityQueryType<E>;
}

impl<E: EntityComponentContext, C: ExternalSystem> SystemList<E, C> for Nil {
    fn execute<'a, T: Executor<'a, E, C>>(
        &'a self,
        _executor: T,
        _context: &'a E,
        _operation_queue: OperationChannel<'a, E>,
        _external: &'a C,
    ) {
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
    fn execute<'a, T: Executor<'a, E, C>>(
        &'a self,
        executor: T,
        context: &'a E,
        operation_queue: OperationChannel<'a, E>,
        external: &'a C,
    ) {
        self.tail
            .execute(executor, context, operation_queue.clone(), external);

        executor.execute(move || {
            context.iter_ref().for_each(|archetype| {
                self.head
                    .execute(archetype, context, &operation_queue, external);
            })
        });
    }

    fn component_write(&self) -> EntityQueryType<E> {
        let head = self.head.component_write();
        let tail = self.tail.component_write();
        head.get_union(tail)
    }
}

impl<
    E: EntityComponentContext,
    M2: Marker,
    C: ExternalSystem,
    S: GlobalSystem<E>,
    N: SystemList<E, C>,
> SystemList<E, C> for Cons<GlobalSystemExecutor<E, M2, C, S>, N>
where
    S::External: Subset<C, M2>,
{
    fn execute<'a, T: Executor<'a, E, C>>(
        &'a self,
        executor: T,
        context: &'a E,
        operation_queue: OperationChannel<'a, E>,
        external: &'a C,
    ) {
        self.tail
            .execute(executor, context, operation_queue.clone(), external);
        executor.execute(move || {
            self.head.execute(context, &operation_queue, external);
        });
    }

    fn component_write(&self) -> EntityQueryType<E> {
        let head = self.head.component_write();
        let tail = self.tail.component_write();
        head.get_union(tail)
    }
}

pub trait Builder<E: EntityComponentContext, C: ExternalSystem> {
    fn with_executor<M1: Marker, M2: Marker, M3: Marker, N: System<E>>(
        self,
        system: SystemExecutor<E, M1, M3, C, N>,
    ) -> impl Builder<E, C>
    where
        N::Components:
            IntoSubsetIterator<ComponentListType<E>, M1> + ComponentQuery<ComponentListType<E>, M1>,
        N::WriteList: ComponentQuery<ComponentListType<E>, M2>,
        N::External: Subset<C, M3>;

    fn with_global_executor<M1: Marker, M2: Marker, N: GlobalSystem<E>>(
        self,
        system: GlobalSystemExecutor<E, M2, C, N>,
    ) -> impl Builder<E, C>
    where
        N::WriteList: ComponentQuery<ComponentListType<E>, M1>,
        N::External: Subset<C, M2>;

    fn component_write(&self) -> EntityQueryType<E>;

    fn build(self) -> impl SystemList<E, C>;
}

pub struct SystemListBuilder<E: EntityComponentContext, C: ExternalSystem, S: SystemList<E, C>> {
    systems: S,
    _marker: PhantomData<(E, C)>,
}

impl<E: EntityComponentContext, C: ExternalSystem> Default for SystemListBuilder<E, C, Nil> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: EntityComponentContext, C: ExternalSystem> SystemListBuilder<E, C, Nil> {
    pub fn new() -> Self {
        SystemListBuilder {
            systems: Nil::new(),
            _marker: PhantomData,
        }
    }
}

impl<E: EntityComponentContext, C: ExternalSystem, S: SystemList<E, C>> Builder<E, C>
    for SystemListBuilder<E, C, S>
{
    fn with_executor<M1: Marker, M2: Marker, M3: Marker, N: System<E>>(
        self,
        system: SystemExecutor<E, M1, M3, C, N>,
    ) -> impl Builder<E, C>
    where
        N::Components:
            IntoSubsetIterator<ComponentListType<E>, M1> + ComponentQuery<ComponentListType<E>, M1>,
        N::WriteList: ComponentQuery<ComponentListType<E>, M2>,
        N::External: Subset<C, M3>,
    {
        SystemListBuilder {
            systems: Cons::new(system, self.systems),
            _marker: PhantomData,
        }
    }

    fn with_global_executor<M1: Marker, M2: Marker, N: GlobalSystem<E>>(
        self,
        system: GlobalSystemExecutor<E, M2, C, N>,
    ) -> impl Builder<E, C>
    where
        N::WriteList: ComponentQuery<ComponentListType<E>, M1>,
        N::External: Subset<C, M2>,
    {
        SystemListBuilder {
            systems: Cons::new(system, self.systems),
            _marker: PhantomData,
        }
    }

    fn component_write(&self) -> EntityQueryType<E> {
        self.systems.component_write()
    }

    fn build(self) -> impl SystemList<E, C> {
        self.systems
    }
}

pub trait GlobalSystem<E: EntityComponentContext>: Sync {
    type External: TypeList;
    type WriteList: TypeList;

    fn execute(
        &self,
        context: &E,
        queue: &OperationChannel<'_, E>,
        external: RefList<'_, Self::External>,
    );

    fn get_entity_update<C: ComponentData, M: Marker>(
        &self,
        index: EntityIndexTyped<E>,
        component: ComponentUpdate<C>,
    ) -> EntityUpdate<E>
    where
        EntityUpdateType<E>: UContains<ComponentUpdate<C>, M>,
    {
        EntityUpdate::new(index, component)
    }
}

pub struct GlobalSystemExecutor<
    E: EntityComponentContext,
    M: Marker,
    C: ExternalSystem,
    S: GlobalSystem<E>,
> where
    S::External: Subset<C, M>,
{
    write: EntityQueryType<E>,
    system: S,
    _phantom: std::marker::PhantomData<(C, M)>,
}

impl<E: EntityComponentContext, M1: Marker, C: ExternalSystem, S: GlobalSystem<E>>
    GlobalSystemExecutor<E, M1, C, S>
where
    S::External: Subset<C, M1>,
{
    #[inline]
    pub fn new<M2: Marker>(system: S) -> Self
    where
        S::WriteList: ComponentQuery<ComponentListType<E>, M2>,
    {
        Self {
            write: <S::WriteList as ComponentQuery<_, _>>::query(),
            system,
            _phantom: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn execute(&self, context: &E, operation_queue: &OperationChannel<'_, E>, external: &C) {
        self.system
            .execute(context, operation_queue, S::External::sub_get(external));
    }

    #[inline]
    pub fn component_write(&self) -> EntityQueryType<E> {
        self.write
    }
}
