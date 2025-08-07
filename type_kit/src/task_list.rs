use std::{any::type_name, convert::Infallible, error::Error, fmt::Debug, marker::PhantomData};

use crate::{Cons, Here, ListMutType, Marked, Marker, Nil, Subset, TypeList, TypedNil};

/// # Safety
/// Task implementator is required to ensure that the ResourceSet associated type
/// does not repeat the same type more than once. This is required so that the `execute`
/// method can safely access the resources without causing the TaskList to create
/// aliased mutable references to the same resource, causing undefined behavior.
pub unsafe trait Task: 'static {
    type Dependencies: DependencyList;
    type ResourceSet: TypeList;
    type InitializerList: TypeList;
    type TaskError: Error;
    type TaskResult;

    fn execute<'a>(
        &'a mut self,
        resources: ListMutType<'a, Self::ResourceSet>,
    ) -> Result<Self::TaskResult, Self::TaskError>;
}

pub struct Dependency<T: Task> {
    _task: PhantomData<T>,
}

impl<T: Task> Debug for Dependency<T> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dependency")
            .field("Task", &type_name::<T>())
            .finish()
    }
}

impl<T: Task> Dependency<T> {
    #[inline]
    pub fn new() -> Self {
        Self { _task: PhantomData }
    }
}

pub trait DependencyList: TypeList {}

impl<T: 'static> DependencyList for TypedNil<T> {}

impl<T: Task, N: DependencyList> DependencyList for Cons<Dependency<T>, N> {}

#[macro_export]
macro_rules! dependency_list {
    [$head:ty] => {
        Cons<Dependency<$head>, Nil>
    };
    [$head:ty $(, $tail:ty)*] => {
        Cons<Dependency<$head>, dependency_list![$($tail),*]>
    };
}

pub trait TaskList<R: TypeList, E: Error>: 'static {
    type TaskResult;
    fn execute(&mut self, resources: &mut R) -> Result<Self::TaskResult, E>;
}

impl<T: 'static, R: TypeList, E: Error> TaskList<R, E> for TypedNil<T> {
    type TaskResult = ();
    #[inline]
    fn execute(&mut self, _resources: &mut R) -> Result<Self::TaskResult, E> {
        Ok(())
    }
}

impl<I: Task, M: Marker, E: Error, R: TypeList> TaskList<R, E> for Marked<I, M>
where
    I::TaskError: Into<E>,
    I::ResourceSet: Subset<R, M>,
{
    type TaskResult = I::TaskResult;

    #[inline]
    fn execute(&mut self, resources: &mut R) -> Result<Self::TaskResult, E> {
        // This is safe because the Task trait guarantees that the ResourceSet does not repeat types.
        self.value
            .execute(unsafe { I::ResourceSet::sub_get_mut(resources) })
            .map_err(Into::into)
    }
}

impl<R: TypeList, E: Error, M: Marker, I: Task, N: TaskList<R, E>> TaskList<R, E>
    for Cons<Marked<I, M>, N>
where
    I::TaskError: Into<E>,
    I::ResourceSet: Subset<R, M>,
{
    type TaskResult = I::TaskResult;

    #[inline]
    fn execute(&mut self, resources: &mut R) -> Result<Self::TaskResult, E> {
        // Task list execute form its tail to head to ensure that the order of execution
        // is the same as order of task insertion in the task list builder.
        self.tail.execute(resources)?;
        let result = self.head.execute(resources)?;
        Ok(result)
    }
}

pub struct ResourceListBuilder<R: TypeList, L: TypeList> {
    resources: R,
    tasks: L,
}

pub struct TaskListBuilder<
    E: Error,
    M: Marker,
    R: TypeList,
    D: DependencyList,
    S: TaskList<R, E>,
    I: Subset<R, M>,
> {
    resources: R,
    stages: S,
    dependencies: D,
    _error: PhantomData<E>,
    _initializer: PhantomData<Marked<I, M>>,
}

impl<R: 'static, T: 'static> ResourceListBuilder<TypedNil<R>, TypedNil<T>> {
    #[inline]
    fn new() -> Self {
        Self {
            resources: TypedNil::new(),
            tasks: TypedNil::new(),
        }
    }

    #[inline]
    pub fn with_resource_terminator_type<N: 'static>(
        self,
    ) -> ResourceListBuilder<TypedNil<N>, TypedNil<T>> {
        ResourceListBuilder::new()
    }

    #[inline]
    pub fn with_task_terminator_type<N: 'static>(
        self,
    ) -> ResourceListBuilder<TypedNil<R>, TypedNil<N>> {
        ResourceListBuilder::new()
    }
}

impl<R: TypeList, T: 'static> ResourceListBuilder<R, TypedNil<T>> {
    #[inline]
    pub fn register_resource<U: 'static>(
        self,
        resource: U,
    ) -> ResourceListBuilder<Cons<U, R>, TypedNil<T>> {
        ResourceListBuilder {
            resources: Cons::new(resource, self.resources),
            tasks: self.tasks,
        }
    }

    #[inline]
    pub fn push_task<I: Task<Dependencies = Nil>, M1: Marker, M2: Marker>(
        self,
        stage: I,
    ) -> TaskListBuilder<
        I::TaskError,
        M2,
        R,
        Cons<Dependency<I>, Nil>,
        Cons<Marked<I, M1>, TypedNil<T>>,
        I::InitializerList,
    >
    where
        I::ResourceSet: Subset<R, M1>,
        I::InitializerList: Subset<R, M2>,
    {
        TaskListBuilder {
            resources: self.resources,
            stages: Cons::new(Marked::new(stage), self.tasks),
            dependencies: Cons::new(Dependency::<I>::new(), TypedNil::new()),
            _error: PhantomData,
            _initializer: PhantomData,
        }
    }
}

impl<
        E: Error,
        M: Marker,
        R: TypeList,
        D: DependencyList,
        S: TaskList<R, E, TaskResult = ()>,
        I: Subset<R, M>,
    > TaskListBuilder<E, M, R, D, S, I>
{
    #[inline]
    pub fn push_task<M1: Marker, M2: Marker, T: Task<InitializerList = Nil>>(
        self,
        stage: T,
    ) -> TaskListBuilder<T::TaskError, M, R, Cons<Dependency<T>, D>, Cons<Marked<T, M1>, S>, I>
    where
        S: TaskList<R, T::TaskError>,
        T::ResourceSet: Subset<R, M1>,
        T::Dependencies: Subset<D, M2>,
    {
        TaskListBuilder {
            resources: self.resources,
            stages: Cons::new(Marked::new(stage), self.stages),
            dependencies: Cons::new(Dependency::<T>::new(), self.dependencies),
            _error: PhantomData,
            _initializer: PhantomData,
        }
    }
}

impl<E: Error, M: Marker, R: TypeList, D: DependencyList, S: TaskList<R, E>, I: Subset<R, M>>
    TaskListBuilder<E, M, R, D, S, I>
{
    #[inline]
    pub fn build(self) -> SynchronousExecutor<E, M, R, I, S> {
        SynchronousExecutor {
            resources: self.resources,
            stages: self.stages,
            _error: PhantomData,
            _initializer: PhantomData,
        }
    }
}

pub struct SynchronousExecutor<E: Error, M: Marker, R: TypeList, I: Subset<R, M>, S: TaskList<R, E>>
{
    resources: R,
    stages: S,
    _error: PhantomData<E>,
    _initializer: PhantomData<Marked<I, M>>,
}

impl SynchronousExecutor<Infallible, Here, Nil, Nil, Nil> {
    #[inline]
    pub fn builder() -> ResourceListBuilder<Nil, Nil> {
        ResourceListBuilder::new()
    }
}

pub trait Executor {
    type InitializerList: TypeList;
    type Resources: TypeList;
    type TaskResult;
    type TaskError: Error;
    type TaskList: TaskList<Self::Resources, Self::TaskError>;

    fn execute(
        &mut self,
        initializer: Self::InitializerList,
    ) -> Result<Self::TaskResult, Self::TaskError>;

    fn into_inner(self) -> (Self::Resources, Self::TaskList);
}

impl<R: TypeList, M: Marker, I: Subset<R, M>, S: TaskList<R, E>, E: Error> Executor
    for SynchronousExecutor<E, M, R, I, S>
{
    type InitializerList = I;
    type Resources = R;
    type TaskResult = S::TaskResult;
    type TaskError = E;
    type TaskList = S;

    #[inline]
    fn execute(
        &mut self,
        initializer: Self::InitializerList,
    ) -> Result<
        <Self::TaskList as TaskList<Self::Resources, Self::TaskError>>::TaskResult,
        Self::TaskError,
    > {
        initializer.sub_write(&mut self.resources);
        self.stages.execute(&mut self.resources)
    }

    #[inline]
    fn into_inner(self) -> (Self::Resources, Self::TaskList) {
        (self.resources, self.stages)
    }
}

#[cfg(test)]
mod test_task_list {
    use std::{convert::Infallible, error::Error, fmt::Display};

    use crate::{
        dependency_list, list_type, list_value, unpack_list, Cons, Dependency, Executor,
        ListMutType, Nil, SynchronousExecutor, Task, TypeList,
    };

    struct Generate;

    unsafe impl Task for Generate {
        type ResourceSet = list_type![Vec<u16>, u16, Nil];
        type InitializerList = list_type![u16, Nil];
        type Dependencies = Nil;
        type TaskError = Infallible;
        type TaskResult = ();

        fn execute<'a>(
            &mut self,
            unpack_list![a, b]: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<(), Self::TaskError> {
            (1..*b).for_each(|i| a.push(i as u16));
            Ok(())
        }
    }

    struct Process;

    unsafe impl Task for Process {
        type ResourceSet = list_type![Vec<u16>, u16, Nil];
        type InitializerList = Nil;
        type Dependencies = dependency_list![Generate];
        type TaskError = Infallible;
        type TaskResult = ();

        fn execute<'a>(
            &mut self,
            unpack_list![vec_u16, sum]: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<Self::TaskResult, Self::TaskError> {
            *sum = vec_u16.iter().sum();
            Ok(())
        }
    }

    struct Cleanup;

    unsafe impl Task for Cleanup {
        type ResourceSet = list_type![Vec<u16>, u16, Nil];
        type InitializerList = Nil;
        type Dependencies = dependency_list![Process];
        type TaskError = Infallible;
        type TaskResult = String;

        fn execute<'a>(
            &mut self,
            unpack_list![a, b]: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<Self::TaskResult, Self::TaskError> {
            let result = format!("ComputedValue: {}", b);
            a.clear();
            *b = 0;
            Ok(result)
        }
    }

    #[test]
    fn test_simple_task() {
        let mut executor = SynchronousExecutor::builder()
            .register_resource(0u16)
            .register_resource(Vec::<u16>::new())
            .push_task(Generate)
            .push_task(Process)
            .push_task(Cleanup)
            .build();
        let result = executor.execute(list_value!(43u16, Nil::new()));
        assert!(result.is_ok());
        let unpack_list![vec_u16, sum] = executor
            .resources
            .sub_ref::<_, list_type![Vec<u16>, u16, Nil]>();
        assert_eq!(*sum, 0u16);
        assert!(vec_u16.is_empty());
        assert_eq!(result.unwrap(), "ComputedValue: 903");
        // Execution stack is reusable
        let result = executor.execute(list_value!(22u16, Nil::new()));
        assert!(result.is_ok());
        let unpack_list![vec_u16, sum] = executor
            .resources
            .sub_ref::<_, list_type![Vec<u16>, u16, Nil]>();
        assert_eq!(*sum, 0u16);
        assert!(vec_u16.is_empty());
        assert_eq!(result.unwrap(), "ComputedValue: 231");
    }

    #[derive(Debug)]
    struct DummyError;

    impl Display for DummyError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "DummyError")
        }
    }

    impl Error for DummyError {}

    struct DummyPassingStage;

    unsafe impl Task for DummyPassingStage {
        type ResourceSet = Nil;
        type InitializerList = Nil;
        type Dependencies = Nil;
        type TaskError = DummyError;
        type TaskResult = ();

        fn execute<'a>(
            &mut self,
            _resources: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<Self::TaskResult, Self::TaskError> {
            Ok(())
        }
    }
    struct FailingStage;

    unsafe impl Task for FailingStage {
        type ResourceSet = Nil;
        type InitializerList = Nil;
        type Dependencies = Nil;
        type TaskError = DummyError;
        type TaskResult = ();

        fn execute<'a>(
            &mut self,
            _resources: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<Self::TaskResult, Self::TaskError> {
            Err(DummyError)
        }
    }

    #[test]
    fn test_failing_stage() {
        let mut stack = SynchronousExecutor::builder()
            .push_task(DummyPassingStage)
            .push_task(FailingStage)
            .build();
        assert!(stack.execute(Nil::new()).is_err());
    }

    struct InfallibleStage;

    unsafe impl Task for InfallibleStage {
        type ResourceSet = Nil;
        type InitializerList = Nil;
        type Dependencies = Nil;
        type TaskError = Infallible;
        type TaskResult = ();

        fn execute<'a>(
            &mut self,
            _resources: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<Self::TaskResult, Self::TaskError> {
            Ok(())
        }
    }

    impl From<Infallible> for DummyError {
        fn from(_: Infallible) -> Self {
            DummyError
        }
    }

    #[test]
    fn test_mixed_error_types() {
        let mut stack = SynchronousExecutor::builder()
            .push_task(InfallibleStage)
            .push_task(FailingStage)
            .build();
        assert!(stack.execute(Nil::new()).is_err());
    }
}
