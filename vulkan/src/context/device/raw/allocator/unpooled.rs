use std::convert::Infallible;

use type_kit::{Create, Destroy, DestroyResult, FromGuard, ScopedInnerMut};

use crate::context::{
    device::{
        raw::{
            allocator::AllocatorIndex,
            resources::{
                memory::{Memory, MemoryAllocateInfo},
                ResourceIndex,
            },
        },
        resources::buffer::ByteRange,
    },
    error::{AllocatorError, ResourceResult},
    Context,
};

use super::{Allocation, AllocationIndex, AllocationRequest, Allocator, Strategy};

pub struct Unpooled {}

impl Create for Unpooled {
    type Config<'a> = ();
    type CreateError = AllocatorError;

    fn create<'a, 'b>(_: Self::Config<'a>, _: Self::Context<'b>) -> type_kit::CreateResult<Self> {
        Ok(Unpooled {})
    }
}

impl Destroy for Unpooled {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, _: Self::Context<'a>) -> DestroyResult<Self> {
        Ok(())
    }
}

impl Strategy for Unpooled {
    type State = ();
    type CreateConfig<'a> = ();

    #[inline]
    fn wrap_index(index: type_kit::GuardIndex<Allocator<Self>>) -> AllocatorIndex {
        AllocatorIndex::Unpooled(index)
    }

    fn allocate<'a>(
        mut allocator: ScopedInnerMut<'a, Allocator<Self>>,
        context: &Context,
        req: AllocationRequest,
    ) -> ResourceResult<AllocationIndex> {
        let alloc_info = MemoryAllocateInfo::new()
            .with_allocation_size(req.requirements.size)
            .with_memory_type_index(context.get_memory_type_index(&req)?);
        let memory: ResourceIndex<Memory> = context.create_resource(alloc_info)?;
        let range = ByteRange::new(req.requirements.size as usize);
        let allocation = Allocation::new(memory, range);
        allocator.memory_map.register(&allocation);
        let index = allocator.allocations.push(allocation.into_guard())?;
        Ok((req.memory_type_info.wrap_index)(index))
    }

    fn free<'a>(
        mut allocator: type_kit::ScopedInnerMut<'a, Allocator<Self>>,
        context: &Context,
        allocation: AllocationIndex,
    ) -> ResourceResult<()> {
        let allocation =
            Allocation::try_from_guard(allocator.allocations.pop(allocation.into_inner())?)
                .map_err(|(_, err)| err)?;
        let memory = allocator.memory_map.pop(allocation)?;
        if let Some(memory) = memory {
            context.destroy_resource(memory)?;
        }
        Ok(())
    }
}
