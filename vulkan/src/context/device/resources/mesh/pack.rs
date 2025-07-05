use std::{any::TypeId, convert::Infallible, marker::PhantomData};

use ash::vk;
use type_kit::{Create, CreateResult, Destroy, DestroyResult};

use crate::context::{
    device::{
        command::operation::{self, Operation},
        memory::AllocReq,
        raw::allocator::AllocatorIndex,
        resources::{
            buffer::{
                Buffer, BufferBuilder, BufferInfo, BufferPartial, Range, StagingBuffer,
                StagingBufferBuilder,
            },
            PartialBuilder,
        },
    },
    error::{VkError, VkResult},
    Context,
};
use graphics::model::{Mesh, Vertex};

use super::{
    BufferRanges, BufferType, MeshByteRange, MeshPackBinding, MeshPackData, MeshPackDataPartial,
};

impl<'a, V: Vertex> PartialBuilder<'a> for MeshPackPartial<'a, V> {
    type Config = &'a [Mesh<V>];
    type Target = MeshPack<V>;

    fn prepare(config: Self::Config, context: &Context) -> VkResult<Self> {
        let num_vertices = config.iter().fold(0, |acc, mesh| acc + mesh.vertices.len());
        let num_indices = config.iter().fold(0, |acc, mesh| acc + mesh.indices.len());
        let mut builder = StagingBufferBuilder::new();
        let vertex_range = builder.append::<V>(num_vertices);
        let index_range = builder.append::<u32>(num_indices);
        let mut buffer_ranges = BufferRanges::new();
        buffer_ranges.set(BufferType::Vertex, vertex_range);
        buffer_ranges.set(BufferType::Index, index_range);
        let buffer = BufferPartial::prepare(
            BufferBuilder::new(BufferInfo {
                size: buffer_ranges.get_rquired_buffer_size(),
                usage: vk::BufferUsageFlags::VERTEX_BUFFER
                    | vk::BufferUsageFlags::INDEX_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST,
                sharing_mode: vk::SharingMode::EXCLUSIVE,
                queue_families: &[operation::Graphics::get_queue_family_index(context)],
            }),
            context,
        )?;
        let partial = MeshPackDataPartial {
            buffer,
            buffer_ranges,
            meshes: config,
        };
        Ok(MeshPackPartial { partial })
    }
    fn requirements(&self) -> impl Iterator<Item = AllocReq> {
        self.partial.buffer.requirements()
    }
}

pub struct MeshPackPartial<'a, V: Vertex> {
    partial: MeshPackDataPartial<'a, V>,
}

#[derive(Debug)]
pub struct MeshPack<V: Vertex> {
    pub data: MeshPackData,
    _phantom: PhantomData<V>,
}

impl<V: Vertex> Create for MeshPack<V> {
    type Config<'a> = (MeshPackPartial<'a, V>, AllocatorIndex);
    type CreateError = VkError;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let (
            MeshPackPartial {
                partial:
                    MeshPackDataPartial {
                        buffer,
                        buffer_ranges,
                        meshes,
                    },
            },
            allocator,
        ) = config;
        let mut buffer = Buffer::create((buffer, allocator), context)?;
        let num_indices = meshes.iter().fold(0, |acc, mesh| acc + mesh.indices.len());
        let num_vertices = meshes.iter().fold(0, |acc, mesh| acc + mesh.vertices.len());
        let mut builder = StagingBufferBuilder::new();
        let vertex_range = builder.append::<V>(num_vertices);
        let index_range = builder.append::<u32>(num_indices);
        let (vertex_ranges, index_ranges) = {
            let mut staging_buffer =
                StagingBuffer::create((builder, context.default_allocator()), &context)?;
            let mut vertex_writer = staging_buffer.write_range::<V>(vertex_range);
            let vertex_ranges = meshes
                .iter()
                .map(|mesh| vertex_writer.write(&mesh.vertices))
                .collect::<Vec<_>>();
            let mut index_writer = staging_buffer.write_range::<u32>(index_range);
            let index_ranges = meshes
                .iter()
                .map(|mesh| index_writer.write(&mesh.indices))
                .collect::<Vec<_>>();
            staging_buffer.transfer_buffer_data(&context, &mut buffer, 0)?;
            let _ = staging_buffer.destroy(&context);
            (vertex_ranges, index_ranges)
        };
        let meshes = vertex_ranges
            .into_iter()
            .zip(index_ranges)
            .map(|(vertices, indices)| MeshByteRange {
                vertices: vertices.into(),
                indices: indices.into(),
            })
            .collect();
        let data = MeshPackData {
            buffer,
            buffer_ranges,
            meshes,
        };
        Ok(MeshPack {
            data,
            _phantom: PhantomData,
        })
    }
}

impl<V: Vertex> Destroy for MeshPack<V> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.data.buffer.destroy(context)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct MeshPackRef<'a, V: Vertex> {
    pub data: &'a MeshPackData,
    pub _phantom: PhantomData<V>,
}

impl<'a, V: Vertex> Clone for MeshPackRef<'a, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, V: Vertex> Copy for MeshPackRef<'a, V> {}

impl<'a, V: Vertex, T: Vertex> TryFrom<&'a MeshPack<V>> for MeshPackRef<'a, T> {
    type Error = &'static str;

    fn try_from(value: &'a MeshPack<V>) -> Result<Self, Self::Error> {
        if TypeId::of::<T>() == TypeId::of::<V>() {
            Ok(Self {
                data: &value.data,
                _phantom: PhantomData,
            })
        } else {
            Err("Invalid Vertex type")
        }
    }
}

impl<'a, V: Vertex> From<MeshPackRef<'a, V>> for MeshPackBinding {
    fn from(value: MeshPackRef<'a, V>) -> Self {
        MeshPackBinding {
            buffer: value.data.buffer.handle(),
            buffer_ranges: value.data.buffer_ranges,
        }
    }
}

impl<'a, V: Vertex> MeshPackRef<'a, V> {
    pub fn get(&self, index: usize) -> MeshRange<V> {
        MeshRange {
            vertices: self.data.meshes[index].vertices.into(),
            indices: self.data.meshes[index].indices.into(),
        }
    }

    pub fn as_raw(&self) -> &MeshPackData {
        self.data
    }
}

impl<'a, V: Vertex> From<&'a MeshPack<V>> for &'a MeshPackData {
    fn from(value: &'a MeshPack<V>) -> Self {
        &value.data
    }
}

impl<'a, V: Vertex> From<&'a mut MeshPack<V>> for &'a mut MeshPackData {
    fn from(value: &'a mut MeshPack<V>) -> Self {
        &mut value.data
    }
}

impl<'a, V: Vertex> From<&'a MeshPack<V>> for MeshPackBinding {
    fn from(value: &'a MeshPack<V>) -> Self {
        (&value.data).into()
    }
}

impl<V: Vertex> MeshPack<V> {
    pub fn get(&self, index: usize) -> MeshRange<V> {
        self.data.meshes[index].into()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MeshRangeBindData {
    pub index_count: u32,
    pub index_offset: u32,
    pub vertex_offset: i32,
}

impl<V: Vertex> From<MeshRange<V>> for MeshRangeBindData {
    fn from(value: MeshRange<V>) -> Self {
        MeshRangeBindData {
            index_count: value.indices.len as u32,
            index_offset: value.indices.first as u32,
            vertex_offset: value.vertices.first as i32,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MeshRange<V: Vertex> {
    pub vertices: Range<V>,
    pub indices: Range<u32>,
}

impl Context {
    pub fn load_mesh_pack<V: Vertex>(
        &self,
        meshes: &[Mesh<V>],
        allocator: AllocatorIndex,
    ) -> VkResult<MeshPack<V>> {
        let partial = MeshPackPartial::prepare(meshes, self)?;
        MeshPack::create((partial, allocator), self)
    }
}
