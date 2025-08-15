mod archetype;
pub mod context;
pub mod entity;
pub mod index;
pub mod operation;
pub mod stage;
pub mod system;

use std::marker::PhantomData;

use type_kit::{Cons, GenVec, IntoCollectionIterator, Nil, StaticTypeList};

use crate::{
    archetype::{Archetype, ArchetypeRef},
    context::EntityComponentContext,
    entity::EntityBuilder,
    index::PersistentIndexMap,
    stage::StageList,
};

pub trait ExternalSystem: StaticTypeList + Sync {}

impl<T: StaticTypeList + Sync> ExternalSystem for T {}

pub trait ComponentData: Send + Sync + 'static {}

impl<T: Send + Sync + 'static> ComponentData for T {}

pub trait ComponentList: IntoCollectionIterator + Send + Sync {}

impl ComponentList for Nil {}

impl<C: ComponentData, N: ComponentList> ComponentList for Cons<GenVec<C>, N> {}

pub trait EntityComponentSystem<E: EntityComponentContext, C: ExternalSystem> {
    fn get_entity_builder(&self) -> EntityBuilder<E>;

    fn add_entity(&mut self, entity: EntityBuilder<E>);

    fn execute_systems(&mut self, external: &C);
}

pub struct EntityComponentSystemContext<
    E: EntityComponentContext,
    C: ExternalSystem,
    S: StageList<E, C>,
> {
    context: E,
    stages: S,
    _marker: PhantomData<C>,
}

impl<E: EntityComponentContext, C: ExternalSystem, S: StageList<E, C>> EntityComponentSystem<E, C>
    for EntityComponentSystemContext<E, C, S>
{
    #[inline]
    fn get_entity_builder(&self) -> EntityBuilder<E> {
        EntityBuilder::new()
    }

    fn add_entity(&mut self, entity: EntityBuilder<E>) {
        self.context.push_entity(entity, None);
    }

    #[inline]
    fn execute_systems(&mut self, external: &C) {
        self.stages.execute(&mut self.context, external);
    }
}

#[cfg(test)]
mod test_ecs {
    use std::{
        fmt::Debug,
        marker::PhantomData,
        sync::{Arc, Mutex},
    };

    use type_kit::{
        list_type, list_value, unpack_list, Cons, GenVec, GenVecIndex, Here, Nil, RefList, There,
        TypeList,
    };

    use crate::{
        component_list_type,
        context::{EntityComponentContext, EntityComponentStorage},
        ecs_context_type, entity_type,
        index::{EntityIndex, PersistentIndex},
        marker_type,
        operation::OperationSender,
        stage::Builder,
        system::System,
        ComponentData, EntityComponentSystem,
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
            unpack_list![borrowed_value]: RefList<'a, Self::Components>,
            context: &EscContextType,
            queue: &OperationSender<EscContextType>,
            _external: RefList<'a, Self::External>,
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
            unpack_list![borrowed_first, borrowed_second]: RefList<'a, Self::Components>,
            _context: &EscContextType,
            _queue: &OperationSender<EscContextType>,
            _external: RefList<'a, Self::External>,
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
            unpack_list![_borrow_u16]: RefList<'a, Self::Components>,
            context: &EscContextType,
            queue: &OperationSender<EscContextType>,
            _external: RefList<'a, Self::External>,
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
            unpack_list![entity_index]: RefList<'a, Self::Components>,
            context: &EscContextType,
            queue: &OperationSender<EscContextType>,
            _external: RefList<'a, Self::External>,
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
            unpack_list![persistent_index]: RefList<'a, Self::Components>,
            context: &EscContextType,
            queue: &OperationSender<EscContextType>,
            _external: RefList<'a, Self::External>,
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
        let external = Nil::new();
        let mut ecs = EscContextType::with_external::<Nil>()
            .with_system(TestSystem::<String>::new())
            .with_system(TestSystem::<u32>::new())
            .with_system(TestSystem::<u16>::new())
            .with_system(TestSystemMulti::<String, u32>::new())
            .with_system(TestEntityQuery)
            .with_system(TestEntityTryGet)
            .with_system(TestEntityPersistentIndex)
            .build();
        let entity = ecs.get_entity_builder().with_component("Hello".to_string());
        ecs.add_entity(entity);
        let entity = ecs
            .get_entity_builder()
            .with_component("World".to_string())
            .with_component::<Option<PersistentIndex>, _>(None);
        ecs.add_entity(entity);
        let entity = ecs
            .get_entity_builder()
            .with_component("The Answer".to_string())
            .with_component(42u32);
        ecs.add_entity(entity);
        let entity = ecs.get_entity_builder().with_component(2u32);
        ecs.add_entity(entity);
        let entity = ecs.get_entity_builder().with_component(1u16);
        ecs.add_entity(entity);
        ecs.execute_systems(&external);

        println!("\n\tECS executed successfully first!\n");

        ecs.execute_systems(&external);

        println!("\n\tECS executed successfully second!\n");

        ecs.execute_systems(&external);

        println!("\n\tECS executed successfully third!\n");
    }

    #[test]
    #[should_panic(expected = "New system's write access is a subset of existing systems")]
    fn test_ecs_stage_write_conflict() {
        let _ = EscContextType::with_external::<Nil>()
            .with_system(TestEntityQuery)
            .with_system(TestEntityQuery)
            .build();
    }

    #[test]
    fn test_ecs_stage_write_conflict_barier() {
        let _ = EscContextType::with_external::<Nil>()
            .with_system(TestEntityQuery)
            .barrier()
            .with_system(TestEntityQuery)
            .build();
    }

    #[test]
    fn test_component_update_on_barrier() {
        let external = Nil::new();
        let mut ecs = EscContextType::with_external::<Nil>()
            .with_system(TestEntityPersistentIndex)
            .barrier()
            .with_system(TestEntityPersistentIndex)
            .build();

        let entity = ecs.get_entity_builder().with_component(1u16);
        ecs.add_entity(entity);
        let entity = ecs
            .get_entity_builder()
            .with_component::<Option<PersistentIndex>, _>(None);
        ecs.add_entity(entity);
        ecs.execute_systems(&external);
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
            unpack_list![component]: RefList<'a, Self::Components>,
            _context: &EscContextType,
            _queue: &OperationSender<EscContextType>,
            unpack_list![external]: RefList<'a, Self::External>,
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
        let external = list_value![ExternalSystem::new(), Nil::new()];
        let mut ecs = EscContextType::with_external::<list_type![ExternalSystem, Nil]>()
            .with_system(TestExternalSystemAcces)
            .build();

        let entity = ecs.get_entity_builder().with_component("Hello".to_string());
        ecs.add_entity(entity);

        let entity = ecs
            .get_entity_builder()
            .with_component("World".to_string())
            .with_component(1u32);
        ecs.add_entity(entity);

        let entity = ecs
            .get_entity_builder()
            .with_component("TheAnswer".to_string())
            .with_component(2u16);
        ecs.add_entity(entity);

        ecs.execute_systems(&external);

        let external_system = external.get::<ExternalSystem, _>();
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
