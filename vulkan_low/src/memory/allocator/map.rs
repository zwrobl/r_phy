use std::{collections::HashMap, convert::Infallible};

use type_kit::{
    BorrowedGuard, Create, Destroy, DestroyResult, DropGuard, FromGuard, GenCollection, TypeGuard,
    TypeGuardVec,
};

use crate::{
    error::{AllocatorError, ResourceError, ResourceResult},
    memory::{
        allocator::{
            Allocation, AllocationBorrow, AllocationIndexTyped, AllocationRaw, MemoryIndex,
            MemoryIndexRaw,
        },
        AllocReqTyped, DeviceLocal, HostCoherent, HostVisible, Memory, MemoryProperties, MemoryRaw,
    },
    Context,
};

#[derive(Debug, Default)]
struct MemoryMap {
    usage: HashMap<TypeGuard<MemoryIndexRaw>, usize>,
    memory: TypeGuardVec<MemoryRaw>,
}

impl MemoryMap {
    #[inline]
    fn new() -> Self {
        Self::default()
    }

    #[inline]
    fn register<M: MemoryProperties>(&mut self, allocation: &Allocation<M>) {
        let memory = allocation.memory.into_guard();
        *self.usage.entry(memory).or_default() += 1;
    }

    #[inline]
    fn pop<M: MemoryProperties>(
        &mut self,
        allocation: Allocation<M>,
    ) -> ResourceResult<Option<DropGuard<Memory<M>>>> {
        let memory = allocation.memory.into_guard();
        let count = self
            .usage
            .get_mut(&memory)
            .ok_or(AllocatorError::InvalidAllocationIndex)?;
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.usage.remove(&memory);
            let memory = self.memory.pop(allocation.memory)?;
            let memory = unsafe { Memory::<M>::from_inner(memory.into_inner()) };
            Ok(Some(DropGuard::new(memory)))
        } else {
            Ok(None)
        }
    }

    #[inline]
    fn borrow<M: MemoryProperties>(
        &mut self,
        allocation: Allocation<M>,
    ) -> ResourceResult<AllocationBorrow<M>> {
        let memory = self.memory.borrow(allocation.memory)?.try_into().unwrap();
        Ok(AllocationBorrow {
            range: allocation.range,
            memory,
        })
    }

    #[inline]
    fn put_back<M: MemoryProperties>(
        &mut self,
        allocation: AllocationBorrow<M>,
    ) -> ResourceResult<()> {
        let AllocationBorrow { memory, .. } = allocation;
        self.memory.put_back(memory.into())?;
        Ok(())
    }

    #[inline]
    fn drain<M: MemoryProperties>(&mut self) -> Vec<MemoryIndex<M>> {
        let (valid, rest): (Vec<_>, Vec<_>) = self
            .usage
            .drain()
            .map(|(memory, count)| {
                MemoryIndex::<M>::try_from_guard(memory).map_err(|(memory, _)| (memory, count))
            })
            .partition(Result::is_ok);
        self.usage = rest.into_iter().map(Result::unwrap_err).collect();
        valid.into_iter().map(Result::unwrap).collect()
    }

    #[inline]
    fn free_memory_type<M: MemoryProperties>(&mut self, context: &Context) {
        let memory_indices = self.drain::<M>();
        memory_indices.into_iter().for_each(|index| {
            let mut memory = self.memory.pop(index).unwrap();
            let _ = memory.destroy(context);
        });
    }

    #[inline]
    fn free_memory(&mut self, context: &Context) {
        self.free_memory_type::<DeviceLocal>(context);
        self.free_memory_type::<HostCoherent>(context);
        self.free_memory_type::<HostVisible>(context);
    }
}

impl Destroy for MemoryMap {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.free_memory(context);
        Ok(())
    }
}

#[derive(Debug)]
pub struct AllocationStore {
    allocations: TypeGuardVec<AllocationRaw>,
    memory_map: MemoryMap,
}

impl Default for AllocationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AllocationStore {
    #[inline]
    pub fn new() -> Self {
        Self {
            allocations: TypeGuardVec::default(),
            memory_map: MemoryMap::new(),
        }
    }

    #[inline]
    pub fn allocate<M: MemoryProperties>(
        &mut self,
        context: &Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<MemoryIndex<M>> {
        let alloc_info = context.get_memory_allocate_info(req)?;
        let memory = Memory::<M>::create(alloc_info, context)?;
        let index = self.memory_map.memory.push(memory.into_guard())?;
        Ok(index)
    }

    #[inline]
    pub fn suballocate<M: MemoryProperties>(
        &mut self,
        req: AllocReqTyped<M>,
        memory: MemoryIndex<M>,
    ) -> ResourceResult<AllocationIndexTyped<M>> {
        let req = req.requirements();
        let mut borrow: BorrowedGuard<Memory<M>, _> =
            self.memory_map.memory.borrow(memory)?.try_into().unwrap();
        let alloc = borrow
            .suballocate(req.size as usize, req.alignment as usize)
            .and_then(|range| Some(Allocation { range, memory }))
            .ok_or(ResourceError::AllocatorError(AllocatorError::OutOfMemory));
        self.memory_map.memory.put_back(borrow.into())?;
        alloc.and_then(|allocation| self.register_allocation(allocation))
    }

    #[inline]
    pub fn register_allocation<M: MemoryProperties>(
        &mut self,
        allocation: Allocation<M>,
    ) -> ResourceResult<AllocationIndexTyped<M>> {
        self.memory_map.register(&allocation);
        let index = self.allocations.push(allocation.into_guard())?;
        Ok(AllocationIndexTyped { index })
    }

    #[inline]
    pub fn pop<M: MemoryProperties>(
        &mut self,
        index: AllocationIndexTyped<M>,
    ) -> ResourceResult<Option<DropGuard<Memory<M>>>> {
        let allocation = self.allocations.pop(index.index)?;
        let allocation = unsafe { Allocation::<M>::from_inner(allocation.into_inner()) };
        self.memory_map.pop(allocation)
    }

    #[inline]
    pub fn borrow<M: MemoryProperties>(
        &mut self,
        index: AllocationIndexTyped<M>,
    ) -> ResourceResult<AllocationBorrow<M>> {
        let allocation =
            Allocation::<M>::try_from_guard(*self.allocations.get(index.index)?).unwrap();
        self.memory_map.borrow(allocation)
    }

    #[inline]
    pub fn put_back<M: MemoryProperties>(
        &mut self,
        allocation: AllocationBorrow<M>,
    ) -> ResourceResult<()> {
        self.memory_map.put_back(allocation)
    }
}

impl Destroy for AllocationStore {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.memory_map.destroy(context);
        Ok(())
    }
}
