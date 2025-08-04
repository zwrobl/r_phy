use std::{collections::HashMap, convert::Infallible, fmt::Debug};

use type_kit::{
    Create, Destroy, DestroyResult, DropGuard, FromGuard, GenCollection, TypeGuard, TypeGuardVec,
};

use crate::{
    error::{AllocatorError, AllocatorResult, ResourceResult},
    memory::{
        allocator::{
            Allocation, AllocationBorrow, AllocationIndexTyped, AllocationRaw, MemoryIndex,
            MemoryIndexRaw,
        },
        range::ByteRange,
        AllocReqTyped, DeviceLocal, HostCoherent, HostVisible, Memory, MemoryProperties, MemoryRaw,
    },
    Context,
};

pub trait MemoryRange: Debug + From<ByteRange> {
    fn try_alloc<M: MemoryProperties>(&mut self, req: &AllocReqTyped<M>) -> Option<ByteRange>;
    fn dealloc(&mut self, _range: ByteRange);
    fn is_empty(&self) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub struct NoReleaseRange {
    range: ByteRange,
    count: usize,
}

impl NoReleaseRange {
    fn new(range: ByteRange) -> Self {
        Self { range, count: 0 }
    }
}

impl From<ByteRange> for NoReleaseRange {
    #[inline]
    fn from(value: ByteRange) -> Self {
        Self::new(value)
    }
}

impl MemoryRange for NoReleaseRange {
    #[inline]
    fn try_alloc<M: MemoryProperties>(&mut self, req: &AllocReqTyped<M>) -> Option<ByteRange> {
        let req = req.requirements();
        if let Some(range) = self
            .range
            .alloc_raw(req.size as usize, req.alignment as usize)
        {
            self.count += 1;
            Some(range)
        } else {
            None
        }
    }

    #[inline]
    fn dealloc(&mut self, _range: ByteRange) {
        self.count = self.count.saturating_sub(1);
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.count == 0
    }
}

#[derive(Debug)]
struct MemoryMap<R: MemoryRange> {
    usage: HashMap<TypeGuard<MemoryIndexRaw>, R>,
    memory: TypeGuardVec<MemoryRaw>,
}

impl<R: MemoryRange> Default for MemoryMap<R> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<R: MemoryRange> MemoryMap<R> {
    #[inline]
    fn new() -> Self {
        Self {
            usage: HashMap::default(),
            memory: TypeGuardVec::default(),
        }
    }

    fn get_usage<M: MemoryProperties>(
        &mut self,
        memory: MemoryIndex<M>,
    ) -> AllocatorResult<&mut R> {
        self.usage
            .get_mut(&memory.into_guard())
            .ok_or(AllocatorError::InvalidAllocationIndex)
    }

    fn pop_usage<M: MemoryProperties>(&mut self, memory: MemoryIndex<M>) -> AllocatorResult<R> {
        self.usage
            .remove(&memory.into_guard())
            .ok_or(AllocatorError::InvalidAllocationIndex)
    }

    #[inline]
    fn try_suballocate<M: MemoryProperties>(
        &mut self,
        memory: MemoryIndex<M>,
        req: &AllocReqTyped<M>,
    ) -> ResourceResult<Allocation<M>> {
        if let Some(range) = self.get_usage::<M>(memory)?.try_alloc(req) {
            Ok(Allocation::new(memory, range))
        } else {
            Err(AllocatorError::OutOfMemory.into())
        }
    }

    #[inline]
    fn push<M: MemoryProperties>(&mut self, memory: Memory<M>) -> ResourceResult<MemoryIndex<M>> {
        let size = memory.size();
        let index = self.memory.push(memory.into_guard())?;
        self.usage
            .insert(index.into_guard(), ByteRange::new(size).into());
        Ok(index)
    }

    #[inline]
    fn pop<M: MemoryProperties>(
        &mut self,
        memory: Allocation<M>,
    ) -> ResourceResult<Option<DropGuard<Memory<M>>>> {
        let Allocation { range, memory } = memory;
        let usage = self.get_usage::<M>(memory)?;
        usage.dealloc(range);
        if usage.is_empty() {
            self.pop_usage::<M>(memory)?;
            // TODO: From now discard the guard if failed to convert
            // In Future error type that can express type conversion failure
            // should be generic over the converted Type and be able to contain the guard type
            let memory =
                Memory::<M>::try_from_guard(self.memory.pop(memory)?).map_err(|(_, err)| err)?;
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

impl<R: MemoryRange> Destroy for MemoryMap<R> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.free_memory(context);
        Ok(())
    }
}

#[derive(Debug)]
pub struct AllocationStore<R: MemoryRange> {
    allocations: TypeGuardVec<AllocationRaw>,
    memory_map: MemoryMap<R>,
}

impl<R: MemoryRange> Default for AllocationStore<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: MemoryRange> AllocationStore<R> {
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
        let index = self.memory_map.push(memory)?;
        Ok(index)
    }

    #[inline]
    pub fn suballocate<M: MemoryProperties>(
        &mut self,
        req: AllocReqTyped<M>,
        memory: MemoryIndex<M>,
    ) -> ResourceResult<AllocationIndexTyped<M>> {
        let allocation = self.memory_map.try_suballocate(memory, &req)?;
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

impl<R: MemoryRange> Destroy for AllocationStore<R> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.memory_map.destroy(context);
        Ok(())
    }
}
