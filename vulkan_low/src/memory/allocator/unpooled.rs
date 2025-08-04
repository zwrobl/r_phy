use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult, GenCell};

use crate::{
    error::{ResourceError, ResourceResult},
    memory::{
        allocator::{AllocationBorrow, AllocationStore, AllocatorIndex, AllocatorIndexTyped},
        AllocReqTyped, MemoryProperties,
    },
    Context,
};

use super::{AllocationIndexTyped, Allocator};

#[derive(Debug)]
pub struct Unpooled {
    store: AllocationStore,
}

impl Default for Unpooled {
    #[inline]
    fn default() -> Self {
        Self {
            store: AllocationStore::new(),
        }
    }
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

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.store.destroy(context)
    }
}

impl Allocator for Unpooled {
    type Storage = GenCell<Self>;

    #[inline]
    fn allocate<'a, M: MemoryProperties>(
        &mut self,
        context: &Context,
        req: AllocReqTyped<M>,
    ) -> ResourceResult<AllocationIndexTyped<M>> {
        let memory = self.store.allocate(context, req)?;
        let allocation = self.store.suballocate(req, memory)?;
        Ok(allocation)
    }

    #[inline]
    fn free<'a, M: MemoryProperties>(
        &mut self,
        context: &Context,
        allocation: AllocationIndexTyped<M>,
    ) -> ResourceResult<()> {
        if let Some(mut memory) = self.store.pop(allocation)? {
            let _ = memory.destroy(context);
        }
        Ok(())
    }

    #[inline]
    fn borrow<M: MemoryProperties>(
        &mut self,
        allocation: AllocationIndexTyped<M>,
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

    #[inline]
    fn wrap_index(index: AllocatorIndexTyped<Self>) -> AllocatorIndex {
        AllocatorIndex::Unpooled(index)
    }
}
