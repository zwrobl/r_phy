use std::{convert::Infallible, error::Error, marker::PhantomData};

use crate::{Cons, ListMutType, Marked, Marker, Nil, Subset, TypeList};

/// # Safety
/// Task implementator is required to ensure that the ResourceSet associated type
/// does not repeat the same type more than once. This is required so that the `execute`
/// method can safely access the resources without causing the TaskExecutor to create
/// aliased mutable references to the same resource, causing undefined behavior.
pub unsafe trait Task: 'static {
    type ResourceSet: TypeList;
    type TaskError: Error;

    fn execute<'a>(
        &'a mut self,
        resources: ListMutType<'a, Self::ResourceSet>,
    ) -> Result<(), Self::TaskError>;
}

// Trait that acts as a layer of indirection to allow for the pipeline stages to be used
// with various compatible resource lists.
// Moving R as the type parameter for the Pipeline trait would make more burdensome to implement
// and less flexible, as it would require the user to specify the type of resource list,
// requiring separate implementations for any compatible resource list, alternatively
// the user could manage with complex trait bounds and blanket implementations.
// Current solution seems to be most flexible and ergonomic.
pub(crate) trait TaskExecutor<R: TypeList, E: Error>: 'static {
    type Marker: Marker;
    type Resources: Subset<R, Self::Marker>;

    fn execute(&mut self, resources: &mut R) -> Result<(), E>;
}

impl<I: Task, M: Marker, E: Error, R: TypeList> TaskExecutor<R, E> for Marked<I, M>
where
    I::TaskError: Into<E>,
    I::ResourceSet: Subset<R, M>,
{
    type Marker = M;
    type Resources = I::ResourceSet;

    #[inline]
    fn execute(&mut self, resources: &mut R) -> Result<(), E> {
        // This is safe because the Task trait guarantees that the ResourceSet does not repeat types.
        self.value
            .execute(unsafe { I::ResourceSet::sub_get_mut(resources) })
            .map_err(Into::into)
    }
}

pub trait TaskList<R: TypeList, E: Error>: 'static {
    fn execute(&mut self, resources: &mut R) -> Result<(), E>;
}

impl<R: TypeList, E: Error> TaskList<R, E> for Nil {
    fn execute(&mut self, _resources: &mut R) -> Result<(), E> {
        Ok(())
    }
}

impl<R: TypeList, E: Error, S: TaskExecutor<R, E>, N: TaskList<R, E>> TaskList<R, E>
    for Cons<S, N>
{
    #[inline]
    fn execute(&mut self, resources: &mut R) -> Result<(), E> {
        // Task list execute form its tail to head to ensure that the order of execution
        // is the same as order of task insertion in the task list builder.
        self.tail.execute(resources)?;
        self.head.execute(resources)?;
        Ok(())
    }
}

pub struct ResourceListBuilder<R: TypeList> {
    resources: R,
}

pub struct TaskListBuilder<R: TypeList, E: Error, S: TaskList<R, E>> {
    resources: R,
    stages: S,
    _error: PhantomData<E>,
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
    pub fn finalize_resource_list(self) -> TaskListBuilder<R, Infallible, Nil> {
        TaskListBuilder {
            resources: self.resources,
            stages: Nil::new(),
            _error: PhantomData,
        }
    }
}

impl<R: TypeList, E: Error, S: TaskList<R, E>> TaskListBuilder<R, E, S> {
    #[inline]
    pub fn push_task<M: Marker, I: Task>(
        self,
        stage: I,
    ) -> TaskListBuilder<R, I::TaskError, Cons<Marked<I, M>, S>>
    where
        S: TaskList<R, I::TaskError>,
        I::ResourceSet: Subset<R, M>,
    {
        TaskListBuilder {
            resources: self.resources,
            stages: Cons::new(Marked::new(stage), self.stages),
            _error: PhantomData,
        }
    }

    #[inline]
    pub fn build(self) -> SynchronousExecutor<E, R, S> {
        SynchronousExecutor {
            resources: self.resources,
            stages: self.stages,
            _error: PhantomData,
        }
    }
}

pub struct SynchronousExecutor<E: Error, R: TypeList, S: TaskList<R, E>> {
    resources: R,
    stages: S,
    _error: PhantomData<E>,
}

impl SynchronousExecutor<Infallible, Nil, Nil> {
    #[inline]
    pub fn builder() -> ResourceListBuilder<Nil> {
        ResourceListBuilder::new()
    }
}

pub trait Executor {
    // Is it usefull to have this information here?
    type Resources: TypeList;
    type TaskError: Error;
    type TaskList: TaskList<Self::Resources, Self::TaskError>;

    fn execute(&mut self) -> Result<(), Self::TaskError>;
}

impl<R: TypeList, S: TaskList<R, E>, E: Error> Executor for SynchronousExecutor<E, R, S> {
    type Resources = R;
    type TaskError = E;
    type TaskList = S;

    #[inline]
    fn execute(&mut self) -> Result<(), Self::TaskError> {
        self.stages.execute(&mut self.resources)
    }
}

#[cfg(test)]
mod test_task_list {
    use std::{convert::Infallible, error::Error, fmt::Display};

    use crate::{list_type, unpack_list, Cons, Executor, ListMutType, Nil, SynchronousExecutor};

    use super::Task;

    struct Generate;

    unsafe impl Task for Generate {
        type ResourceSet = list_type![Vec<u16>, Nil];
        type TaskError = Infallible;

        fn execute<'a>(
            &mut self,
            unpack_list![a]: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<(), Self::TaskError> {
            println!("Begin: Generate; resources: Vec<u16>: {:?}", a);
            (1..43).for_each(|i| a.push(i as u16));
            println!("End: Generate; resources: Vec<u16>: {:?}", a);
            Ok(())
        }
    }

    struct Process;

    unsafe impl Task for Process {
        type ResourceSet = list_type![Vec<u16>, u16, Nil];
        type TaskError = Infallible;

        fn execute<'a>(
            &mut self,
            unpack_list![vec_u16, sum]: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<(), Self::TaskError> {
            println!(
                "Begin: StageB; resources: Vec<u16>: {:?}, sum: {}",
                vec_u16, sum
            );
            *sum = vec_u16.iter().sum();
            println!(
                "End: StageB; resources: Vec<u16>: {:?}, sum: {}",
                vec_u16, sum
            );
            Ok(())
        }
    }

    struct Cleanup;

    unsafe impl Task for Cleanup {
        type ResourceSet = list_type![Vec<u16>, u16, String, Nil];
        type TaskError = Infallible;

        fn execute<'a>(
            &mut self,
            unpack_list![a, b, c]: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<(), Self::TaskError> {
            println!(
                "Begin: StageC; resources: Vec<u16>: {:?}, u16: {}, String: {}",
                a, b, c
            );
            *c = format!("ComputedValue: {}", b);
            a.clear();
            *b = 0;
            println!(
                "End: StageC; resources: Vec<u16>: {:?}, u16: {}, String: {}",
                a, b, c
            );
            Ok(())
        }
    }

    #[test]
    fn test_empty_task_list() {
        let mut stack = SynchronousExecutor::builder()
            .finalize_resource_list()
            .build();
        assert!(stack.execute().is_ok());
    }

    #[test]
    fn test_simple_task() {
        let mut stack = SynchronousExecutor::builder()
            .register_resource(0u16)
            .register_resource(Vec::<u16>::new())
            .register_resource("Hello".to_owned())
            .finalize_resource_list()
            .push_task(Generate)
            .push_task(Process)
            .push_task(Cleanup)
            .build();
        assert!(stack.execute().is_ok());
        let unpack_list![string, vec_u16, sum] = stack.resources;
        assert_eq!(sum, 0u16);
        assert!(vec_u16.is_empty());
        assert_eq!(string, "ComputedValue: 903");
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
        type TaskError = DummyError;

        fn execute<'a>(
            &mut self,
            _resources: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<(), Self::TaskError> {
            Ok(())
        }
    }
    struct FailingStage;

    unsafe impl Task for FailingStage {
        type ResourceSet = Nil;
        type TaskError = DummyError;

        fn execute<'a>(
            &mut self,
            _resources: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<(), Self::TaskError> {
            Err(DummyError)
        }
    }

    #[test]
    fn test_failing_stage() {
        let mut stack = SynchronousExecutor::builder()
            .finalize_resource_list()
            .push_task(DummyPassingStage)
            .push_task(FailingStage)
            .build();
        assert!(stack.execute().is_err());
    }

    struct InfallibleStage;

    unsafe impl Task for InfallibleStage {
        type ResourceSet = Nil;
        type TaskError = Infallible;

        fn execute<'a>(
            &mut self,
            _resources: ListMutType<'a, Self::ResourceSet>,
        ) -> Result<(), Self::TaskError> {
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
            .finalize_resource_list()
            .push_task(InfallibleStage)
            .push_task(FailingStage)
            .build();
        assert!(stack.execute().is_err());
    }
}
