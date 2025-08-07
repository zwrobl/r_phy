use std::{any::type_name, convert::Infallible, error::Error, fmt::Debug, marker::PhantomData};

use crate::{Cons, Here, ListMutType, Marked, Marker, Nil, Subset, TypeList};

/// # Safety
/// Task implementator is required to ensure that the ResourceSet associated type
/// does not repeat the same type more than once. This is required so that the `execute`
/// method can safely access the resources without causing the TaskExecutor to create
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

impl DependencyList for Nil {}

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

// Trait that acts as a layer of indirection to allow for the pipeline stages to be used
// with various compatible resource lists.
// Moving R as the type parameter for the Pipeline trait would make more burdensome to implement
// and less flexible, as it would require the user to specify the type of resource list,
// requiring separate implementations for any compatible resource list, alternatively
// the user could manage with complex trait bounds and blanket implementations.
// Current solution seems to be most flexible and ergonomic.
pub trait TaskExecutor<R: TypeList, E: Error>: 'static {
    type TaskResult;
    type Marker: Marker;
    type Resources: Subset<R, Self::Marker>;

    fn execute(&mut self, resources: &mut R) -> Result<Self::TaskResult, E>;
}

impl<I: Task, M: Marker, E: Error, R: TypeList> TaskExecutor<R, E> for Marked<I, M>
where
    I::TaskError: Into<E>,
    I::ResourceSet: Subset<R, M>,
{
    type Marker = M;
    type TaskResult = I::TaskResult;
    type Resources = I::ResourceSet;

    #[inline]
    fn execute(&mut self, resources: &mut R) -> Result<Self::TaskResult, E> {
        // This is safe because the Task trait guarantees that the ResourceSet does not repeat types.
        self.value
            .execute(unsafe { I::ResourceSet::sub_get_mut(resources) })
            .map_err(Into::into)
    }
}

pub trait TaskList<R: TypeList, E: Error>: 'static {
    type TaskResult;
    fn execute(&mut self, resources: &mut R) -> Result<Self::TaskResult, E>;
}

impl<R: TypeList, E: Error> TaskList<R, E> for Nil {
    type TaskResult = ();
    fn execute(&mut self, _resources: &mut R) -> Result<Self::TaskResult, E> {
        Ok(())
    }
}

impl<R: TypeList, E: Error, S: TaskExecutor<R, E>, N: TaskList<R, E>> TaskList<R, E>
    for Cons<S, N>
{
    type TaskResult = S::TaskResult;

    #[inline]
    fn execute(&mut self, resources: &mut R) -> Result<Self::TaskResult, E> {
        // Task list execute form its tail to head to ensure that the order of execution
        // is the same as order of task insertion in the task list builder.
        self.tail.execute(resources)?;
        let result = self.head.execute(resources)?;
        Ok(result)
    }
}

pub struct ResourceListBuilder<R: TypeList> {
    resources: R,
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

impl ResourceListBuilder<Nil> {
    #[inline]
    fn new() -> Self {
        Self {
            resources: Nil::new(),
        }
    }
}

impl<R: TypeList> ResourceListBuilder<R> {
    #[inline]
    pub fn register_resource<T: 'static>(self, resource: T) -> ResourceListBuilder<Cons<T, R>> {
        ResourceListBuilder {
            resources: Cons::new(resource, self.resources),
        }
    }

    #[inline]
    pub fn push_task<T: Task<Dependencies = Nil>, M1: Marker, M2: Marker>(
        self,
        stage: T,
    ) -> TaskListBuilder<
        T::TaskError,
        M2,
        R,
        Cons<Dependency<T>, Nil>,
        Cons<Marked<T, M1>, Nil>,
        T::InitializerList,
    >
    where
        T::ResourceSet: Subset<R, M1>,
        T::InitializerList: Subset<R, M2>,
    {
        TaskListBuilder {
            resources: self.resources,
            stages: Cons::new(Marked::new(stage), Nil::new()),
            dependencies: Cons::new(Dependency::<T>::new(), Nil::new()),
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
    pub fn builder() -> ResourceListBuilder<Nil> {
        ResourceListBuilder::new()
    }
}

pub trait Executor {
    // Is it usefull to have this information here?
    type Marker: Marker;
    type Resources: TypeList;
    type TaskError: Error;
    type TaskList: TaskList<Self::Resources, Self::TaskError>;
    type InitializerList: Subset<Self::Resources, Self::Marker>;

    fn execute(
        &mut self,
        initializer: Self::InitializerList,
    ) -> Result<
        <Self::TaskList as TaskList<Self::Resources, Self::TaskError>>::TaskResult,
        Self::TaskError,
    >;
}

impl<R: TypeList, M: Marker, I: Subset<R, M>, S: TaskList<R, E>, E: Error> Executor
    for SynchronousExecutor<E, M, R, I, S>
{
    type Marker = M;
    type Resources = R;
    type TaskError = E;
    type TaskList = S;
    type InitializerList = I;

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
}

#[cfg(test)]
mod test_task_list {
    use std::{convert::Infallible, error::Error, fmt::Display};

    use crate::{
        dependency_list, list_type, list_value, unpack_list, Cons, Dependency, Executor,
        ListMutType, Nil, SynchronousExecutor, Task,
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
        let mut stack = SynchronousExecutor::builder()
            .register_resource(0u16)
            .register_resource(Vec::<u16>::new())
            .push_task(Generate)
            .push_task(Process)
            .push_task(Cleanup)
            .build();
        let result = stack.execute(list_value!(43u16, Nil::new()));
        assert!(result.is_ok());
        let unpack_list![vec_u16, sum] = stack.resources;
        assert_eq!(sum, 0u16);
        assert!(vec_u16.is_empty());
        assert_eq!(result.unwrap(), "ComputedValue: 903");
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
