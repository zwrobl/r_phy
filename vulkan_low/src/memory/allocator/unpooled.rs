use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult};

use crate::{
    error::{ResourceError, ResourceResult},
    memory::{
        allocator::{Allocation, AllocationStore, AllocatorInstance},
        range::ByteRange,
        AllocReqTyped, MemoryProperties,
    },
    resources::{memory::Memory, ResourceIndex},
    Context,
};

use super::{AllocationIndex, Allocator};

#[derive(Debug)]
pub struct Unpooled {
    store: AllocationStore,
}

impl Create for Unpooled {
    type Config<'a> = ();
    type CreateError = ResourceError;

    fn create<'a, 'b>(_: Self::Config<'a>, _: Self::Context<'b>) -> type_kit::CreateResult<Self> {
        Ok(Unpooled {
            store: AllocationStore::new(),
        })
    }
}

impl Destroy for Unpooled {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, _: Self::Context<'a>) -> DestroyResult<Self> {
        Ok(())
    }
}

impl From<Unpooled> for AllocatorInstance {
    #[inline]
    fn from(value: Unpooled) -> Self {
        AllocatorInstance::Unpooled(value)
    }
}

impl Allocator for Unpooled {
    #[inline]
    fn allocate<'a, M: MemoryProperties>(
        &mut self,
        context: &Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationIndex<M>> {
        let requirements = req.requirements();
        let alloc_info = context.get_memory_allocate_info(req)?;
        let memory: ResourceIndex<Memory<M>> = context.create_resource(alloc_info)?;
        let range = ByteRange::new(requirements.size as usize);
        let index = self.store.push(Allocation::new(memory, range))?;
        Ok(index)
    }

    #[inline]
    fn free<'a, M: MemoryProperties>(
        &mut self,
        context: &Context,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<()> {
        if let Some(memory) = self.store.pop(allocation)? {
            context.destroy_resource(memory)?;
        }
        Ok(())
    }

    #[inline]
    fn get_allocation<M: MemoryProperties>(
        &self,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<Allocation<M>> {
        self.store.get_allocation(allocation)
    }
}
