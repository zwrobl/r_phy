use crate::{Cons, ListMutType, Marked, Marker, Nil, Subset, TypeList};

/// # Safety
/// Task implementator is required to ensure that the ResourceSet associated type
/// does not repeat the same type more than once. This is required so that the `execute`
/// method can safely access the resources without causing the TaskExecutor to create
/// aliased mutable references to the same resource, causing undefined behavior.
pub unsafe trait Task: 'static {
    type ResourceSet: TypeList;

    fn execute<'a>(&'a mut self, resources: ListMutType<'a, Self::ResourceSet>);
}

// Trait that acts as a layer of indirection to allow for the pipeline stages to be used
// with various compatible resource lists.
// Moving R as the type parameter for the Pipeline trait would make more burdensome to implement
// and less flexible, as it would require the user to specify the type of resource list,
// requiring separate implementations for any compatible resource list, alternatively
// the user could manage with complex trait bounds and blanket implementations.
// Current solution seems to be most flexible and ergonomic.
pub(crate) trait TaskExecutor<R: TypeList>: 'static {
    type Marker: Marker;
    type Resources: Subset<R, Self::Marker>;

    fn execute(&mut self, resources: &mut R);
}

impl<I: Task, M: Marker, R: TypeList> TaskExecutor<R> for Marked<I, M>
where
    I::ResourceSet: Subset<R, M>,
{
    type Marker = M;
    type Resources = I::ResourceSet;

    #[inline]
    fn execute(&mut self, resources: &mut R) {
        // This is safe because the Task trait guarantees that the ResourceSet does not repeat types.
        self.value
            .execute(unsafe { I::ResourceSet::sub_get_mut(resources) });
    }
}

pub trait TaskList<R: TypeList>: 'static {
    fn execute(&mut self, resources: &mut R);
}

impl<R: TypeList> TaskList<R> for Nil {
    fn execute(&mut self, _resources: &mut R) {}
}

impl<R: TypeList, S: TaskExecutor<R>, N: TaskList<R>> TaskList<R> for Cons<S, N> {
    #[inline]
    fn execute(&mut self, resources: &mut R) {
        // Task list execute form its tail to head to ensure that the order of execution
        // is the same as order of task insertion in the task list builder.
        self.tail.execute(resources);
        self.head.execute(resources);
    }
}

pub struct ResourceListBuilder<R: TypeList> {
    resources: R,
}

pub struct TaskListBuilder<R: TypeList, S: TaskList<R>> {
    resources: R,
    stages: S,
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
    pub fn finalize_resource_list(self) -> TaskListBuilder<R, Nil> {
        TaskListBuilder {
            resources: self.resources,
            stages: Nil::new(),
        }
    }
}

impl<R: TypeList, S: TaskList<R>> TaskListBuilder<R, S> {
    #[inline]
    pub fn push_task<M: Marker, I: Task>(
        self,
        stage: I,
    ) -> TaskListBuilder<R, Cons<Marked<I, M>, S>>
    where
        for<'a> I::ResourceSet: Subset<R, M>,
    {
        TaskListBuilder {
            resources: self.resources,
            stages: Cons::new(Marked::new(stage), self.stages),
        }
    }

    #[inline]
    pub fn build(self) -> SynchronousExecutor<R, S> {
        SynchronousExecutor {
            resources: self.resources,
            stages: self.stages,
        }
    }
}

pub struct SynchronousExecutor<R: TypeList, S: TaskList<R>> {
    resources: R,
    stages: S,
}

impl SynchronousExecutor<Nil, Nil> {
    #[inline]
    pub fn builder() -> ResourceListBuilder<Nil> {
        ResourceListBuilder::new()
    }
}

pub trait Executor {
    // Is it usefull to have this information here?
    type Resources: TypeList;
    type TaskList: TaskList<Self::Resources>;

    fn execute(&mut self);
}

impl<R: TypeList, S: TaskList<R>> Executor for SynchronousExecutor<R, S> {
    type Resources = R;
    type TaskList = S;

    #[inline]
    fn execute(&mut self) {
        self.stages.execute(&mut self.resources);
    }
}

#[cfg(test)]
mod test_pipeline_stack {
    use crate::{list_type, unpack_list, Cons, Executor, ListMutType, Nil, SynchronousExecutor};

    use super::Task;

    struct Generate;

    unsafe impl Task for Generate {
        type ResourceSet = list_type![Vec<u16>, Nil];

        fn execute<'a>(&mut self, unpack_list![a]: ListMutType<'a, Self::ResourceSet>) {
            println!("Begin: Generate; resources: Vec<u16>: {:?}", a);
            (1..43).for_each(|i| a.push(i as u16));
            println!("End: Generate; resources: Vec<u16>: {:?}", a);
        }
    }

    struct Process;

    unsafe impl Task for Process {
        type ResourceSet = list_type![Vec<u16>, u16, Nil];

        fn execute<'a>(&mut self, unpack_list![vec_u16, sum]: ListMutType<'a, Self::ResourceSet>) {
            println!(
                "Begin: StageB; resources: Vec<u16>: {:?}, sum: {}",
                vec_u16, sum
            );
            *sum = vec_u16.iter().sum();
            println!(
                "End: StageB; resources: Vec<u16>: {:?}, sum: {}",
                vec_u16, sum
            );
        }
    }

    struct Cleanup;

    unsafe impl Task for Cleanup {
        type ResourceSet = list_type![Vec<u16>, u16, String, Nil];

        fn execute<'a>(&mut self, unpack_list![a, b, c]: ListMutType<'a, Self::ResourceSet>) {
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
        }
    }

    #[test]
    fn test_empty_task_list() {
        let mut stack = SynchronousExecutor::builder()
            .finalize_resource_list()
            .build();
        stack.execute();
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
        stack.execute();
        let unpack_list![string, vec_u16, sum] = stack.resources;
        assert_eq!(sum, 0u16);
        assert!(vec_u16.is_empty());
        assert_eq!(string, "ComputedValue: 903");
    }
}
