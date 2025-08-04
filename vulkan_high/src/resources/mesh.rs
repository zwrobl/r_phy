mod list;
mod pack;

pub use list::*;
pub use pack::*;
use type_kit::{unpack_list, Cons, DropGuard};

use std::ops::Index;

use strum::EnumCount;

use graphics::model::{Mesh, Vertex};

use vulkan_low::{
    index_list,
    memory::{range::ByteRange, DeviceLocal},
    resources::{
        buffer::{Buffer, BufferPartial},
        command::{level::Level, operation::Operation, BufferBinding, IndexType, RecordingCommand},
        storage::ResourceIndexListBuilder,
        ResourceIndex,
    },
    Context,
};

#[derive(strum::EnumCount)]
pub enum BufferType {
    Vertex,
    Index,
}

#[derive(Debug, Clone, Copy)]
pub struct BufferRanges {
    ranges: [Option<ByteRange>; BufferType::COUNT],
}

impl Index<BufferType> for BufferRanges {
    type Output = ByteRange;
    fn index(&self, index: BufferType) -> &Self::Output {
        self.ranges[index as usize]
            .as_ref()
            .expect("Required bufer data not present!")
    }
}

impl BufferRanges {
    fn new() -> Self {
        Self {
            ranges: [None; BufferType::COUNT],
        }
    }

    fn get_rquired_buffer_size(&self) -> usize {
        self.ranges
            .iter()
            .filter_map(|&range| range)
            .max_by_key(|range| range.end)
            .unwrap()
            .end
    }

    fn set(&mut self, buffer_type: BufferType, range: impl Into<ByteRange>) {
        self.ranges[buffer_type as usize] = Some(range.into());
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MeshByteRange {
    pub vertices: ByteRange,
    pub indices: ByteRange,
}

impl<V: Vertex> From<MeshByteRange> for MeshRange<V> {
    fn from(value: MeshByteRange) -> Self {
        Self {
            vertices: value.vertices.into(),
            indices: value.indices.into(),
        }
    }
}

pub struct MeshPackDataPartial<'a, V: Vertex> {
    meshes: &'a [Mesh<V>],
    buffer_ranges: BufferRanges,
    buffer: DropGuard<BufferPartial<DeviceLocal>>,
}

#[derive(Debug)]
pub struct MeshPackData {
    buffer: ResourceIndex<Buffer<DeviceLocal>>,
    buffer_ranges: BufferRanges,
    meshes: Vec<MeshByteRange>,
}

// impl<'a> From<&'a mut MeshPackData> for &'a mut Buffer<DeviceLocal> {
//     fn from(value: &'a mut MeshPackData) -> Self {
//         (&mut value.buffer).into()
//     }
// }

#[derive(Debug, Clone, Copy)]
pub struct MeshPackBinding {
    pub buffer: ResourceIndex<Buffer<DeviceLocal>>,
    pub buffer_ranges: BufferRanges,
}

impl<'a> From<&'a MeshPackData> for MeshPackBinding {
    fn from(value: &'a MeshPackData) -> Self {
        Self {
            buffer: value.buffer,
            buffer_ranges: value.buffer_ranges,
        }
    }
}

pub fn bind_mesh_pack<'a, T, L: Level, O: Operation>(
    context: &Context,
    command: RecordingCommand<'a, T, L, O>,
    pack: impl Into<MeshPackBinding>,
) -> RecordingCommand<'a, T, L, O> {
    let pack = pack.into();
    context
        .operate_ref(index_list![pack.buffer], |unpack_list![buffer]| {
            command
                .bind_index_buffer(
                    BufferBinding {
                        buffer: buffer.get_vk_buffer(),
                        range: pack.buffer_ranges[BufferType::Index],
                    },
                    IndexType::U32,
                )
                .bind_vertex_buffer(BufferBinding {
                    buffer: buffer.get_vk_buffer(),
                    range: pack.buffer_ranges[BufferType::Vertex],
                })
        })
        .unwrap()
}
