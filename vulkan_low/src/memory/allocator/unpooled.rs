use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult};

use crate::{
    error::{ResourceError, ResourceResult},
    memory::{
        allocator::{AllocationBorrow, AllocationStore, AllocatorInstance},
        AllocReqTyped, MemoryProperties,
    },
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
        let memory = self.store.allocate(context, req)?;
        let allocation = self.store.suballocate(req, memory)?;
        Ok(allocation)
    }

    #[inline]
    fn free<'a, M: MemoryProperties>(
        &mut self,
        context: &Context,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<()> {
        if let Some(mut memory) = self.store.pop(allocation)? {
            let _ = memory.destroy(context);
        }
        Ok(())
    }

    #[inline]
    fn borrow<M: MemoryProperties>(
        &mut self,
        allocation: AllocationIndex<M>,
    ) -> ResourceResult<AllocationBorrow<M>> {
        self.store.borrow(allocation)
    }

    #[inline]
    fn put_back<M: MemoryProperties>(
        &mut self,
        allocation: AllocationBorrow<M>,
    ) -> ResourceResult<()> {
        self.store.put_back(allocation)
    }
}
