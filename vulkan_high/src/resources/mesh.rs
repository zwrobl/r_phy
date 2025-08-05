mod list;
mod pack;

pub use list::*;
pub use pack::*;
use type_kit::DropGuard;

use std::ops::Index;

use strum::EnumCount;

use graphics::model::{Mesh, Vertex};

use vulkan_low::{
    memory::{range::ByteRange, DeviceLocal},
    resources::{
        buffer::{Buffer, BufferPartial},
        command::{
            BindIndexBuffer, BindVertexBuffer, Level, Lifetime, Operation, Recorder,
            RecordingCommand,
        },
        ResourceIndex,
    },
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

#[derive(Debug, Clone, Copy)]
pub struct PackBufferBindings {
    vertex: BindVertexBuffer,
    index: BindIndexBuffer,
}

impl PackBufferBindings {
    #[inline]
    fn new(vertex: BindVertexBuffer, index: BindIndexBuffer) -> Self {
        Self { vertex, index }
    }
}

#[derive(Debug)]
pub struct MeshPackData {
    bindings: PackBufferBindings,
    buffer: ResourceIndex<Buffer<DeviceLocal>>,
    meshes: Vec<MeshByteRange>,
    _buffer_ranges: BufferRanges,
}

impl Recorder for PackBufferBindings {
    fn record<'a, 'b, T: Lifetime, L: Level, O: Operation>(
        &self,
        command: RecordingCommand<'a, T, L, O>,
    ) -> RecordingCommand<'a, T, L, O> {
        command.push(&self.index).push(&self.vertex)
    }
}
