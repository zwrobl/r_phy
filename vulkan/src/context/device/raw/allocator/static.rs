use std::{convert::Infallible, ops::BitAndAssign};

use ash::vk;
use strum::EnumCount;
use type_kit::{Create, CreateResult, Destroy, DestroyResult};

use crate::context::{
    device::{
        memory::MemoryType,
        raw::{
            allocator::{AllocReq, Allocation, AllocationStore, Allocator, AllocatorInstance},
            resources::{memory::Memory, ResourceIndex},
        },
        resources::buffer::ByteRange,
    },
    error::{AllocatorError, ResourceError, ResourceResult},
    Context,
};

use super::AllocationIndex;

#[derive(Debug, Clone, Copy)]
struct LinearBuffer {
    memory: ResourceIndex<Memory>,
    range: ByteRange,
    memory_type_bits: u32,
}

#[derive(Debug)]
pub struct StaticConfig {
    buffers: [(ByteRange, u32); MemoryType::COUNT],
}

impl StaticConfig {
    #[inline]
    pub fn new() -> Self {
        Self {
            buffers: [(ByteRange::empty(), u32::MAX); MemoryType::COUNT],
        }
    }

    #[inline]
    pub fn push_allocation<R: Into<AllocReq>>(&mut self, req: R) -> &mut Self {
        let req: AllocReq = req.into();
        let requirements = req.requirements();
        let memory_type = req.get_memory_type();
        let (range, memory_type_bits) = &mut self.buffers[memory_type as usize];
        range.extend_raw(requirements.size as usize, requirements.alignment as usize);
        memory_type_bits.bitand_assign(requirements.memory_type_bits);
        self
    }
}

#[derive(Debug)]
pub struct Static {
    buffers: [Option<LinearBuffer>; MemoryType::COUNT],
    store: AllocationStore,
}

impl Static {}

impl Create for Static {
    type Config<'a> = StaticConfig;
    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let StaticConfig { buffers: config } = config;
        let buffers = config.iter().enumerate().try_fold(
            [None; MemoryType::COUNT],
            |mut buffers, (i, &(range, memory_type_bits))| {
                if range.len() != 0 && memory_type_bits != 0 {
                    let req =
                        MemoryType::from_repr(i)
                            .unwrap()
                            .get_alloc_req(vk::MemoryRequirements {
                                size: range.len() as vk::DeviceSize,
                                alignment: 0,
                                memory_type_bits,
                            });
                    let alloc_info = context.get_memory_allocate_info(req)?;
                    let memory = context.create_resource(alloc_info)?;
                    buffers[i] = Some(LinearBuffer {
                        memory,
                        range,
                        memory_type_bits,
                    });
                }
                Result::<_, ResourceError>::Ok(buffers)
            },
        )?;
        Ok(Self {
            buffers,
            store: AllocationStore::new(),
        })
    }
}

impl Destroy for Static {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.buffers.iter().for_each(|buffer| {
            if let Some(buffer) = buffer {
                let _ = context.destroy_resource(buffer.memory);
            }
        });
        Ok(())
    }
}

impl From<Static> for AllocatorInstance {
    #[inline]
    fn from(value: Static) -> Self {
        AllocatorInstance::Static(value)
    }
}

impl Allocator for Static {
    #[inline]
    fn allocate<'a>(
        &mut self,
        _context: &crate::Context,
        req: AllocReq,
    ) -> ResourceResult<AllocationIndex> {
        if let Some(buffer) = &mut self.buffers[req.get_memory_type() as usize] {
            let requirements = req.requirements();
            if (buffer.memory_type_bits & requirements.memory_type_bits) != 0 {
                let range = buffer
                    .range
                    .alloc_raw(requirements.size as usize, requirements.alignment as usize)
                    .ok_or(AllocatorError::OutOfMemory)?;
                return self.store.push(Allocation::new(buffer.memory, range));
            }
        };
        Err(ResourceError::AllocatorError(
            AllocatorError::UnsupportedMemoryType,
        ))
    }

    #[inline]
    fn free<'a>(
        &mut self,
        _context: &crate::Context,
        allocation: AllocationIndex,
    ) -> ResourceResult<()> {
        self.store.pop(allocation).map(|_| ())
    }

    #[inline]
    fn get_allocation(&self, allocation: AllocationIndex) -> ResourceResult<Allocation> {
        self.store.get_allocation(allocation)
    }
}
