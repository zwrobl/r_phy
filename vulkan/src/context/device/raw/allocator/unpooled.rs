use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult};

use crate::context::{
    device::{
        raw::{
            allocator::{AllocReq, Allocation, AllocationStore, AllocatorInstance},
            resources::{memory::Memory, ResourceIndex},
        },
        resources::buffer::ByteRange,
    },
    error::{ResourceError, ResourceResult},
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
    fn allocate<'a>(
        &mut self,
        context: &Context,
        req: AllocReq,
    ) -> ResourceResult<AllocationIndex> {
        let requirements = req.requirements();
        let alloc_info = context.get_memory_allocate_info(req)?;
        let memory: ResourceIndex<Memory> = context.create_resource(alloc_info)?;
        let range = ByteRange::new(requirements.size as usize);
        let index = self.store.push(Allocation::new(memory, range))?;
        Ok(index)
    }

    #[inline]
    fn free<'a>(&mut self, context: &Context, allocation: AllocationIndex) -> ResourceResult<()> {
        if let Some(memory) = self.store.pop(allocation)? {
            context.destroy_resource(memory)?;
        }
        Ok(())
    }

    #[inline]
    fn get_allocation(&self, allocation: AllocationIndex) -> ResourceResult<Allocation> {
        self.store.get_allocation(allocation)
    }
}
