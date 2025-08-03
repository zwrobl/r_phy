use std::{marker::PhantomData, mem::offset_of, ops::Deref};

use bytemuck::{Pod, Zeroable};

use math::types::{Vector2, Vector3, Vector4};
use physics::shape;
use type_kit::{Cons, Nil, TypedNil};

pub struct Component {
    pub size: usize,
    pub offset: usize,
}

pub trait Vertex: Pod + Zeroable {
    fn pos(&mut self) -> &mut Vector3;
    fn components() -> &'static [Component];
}

#[derive(Debug)]
pub struct MeshHandle<V: Vertex> {
    index: u32,
    _marker: PhantomData<V>,
}

impl<V: Vertex> Clone for MeshHandle<V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V: Vertex> Copy for MeshHandle<V> {}

impl<V: Vertex> MeshHandle<V> {
    pub fn new(index: u32) -> Self {
        Self {
            index,
            _marker: PhantomData,
        }
    }

    pub fn index(&self) -> u32 {
        self.index
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Zeroable, Pod)]
pub struct CommonVertex {
    pub(crate) pos: Vector3,
    pub(crate) color: Vector3,
    pub(crate) norm: Vector3,
    pub(crate) uv: Vector2,
    pub(crate) tan: Vector4,
}

impl Vertex for CommonVertex {
    fn pos(&mut self) -> &mut Vector3 {
        &mut self.pos
    }

    fn components() -> &'static [Component] {
        const COMPONENTS: &[Component] = &[
            Component {
                size: size_of::<Vector3>(),
                offset: offset_of!(CommonVertex, pos),
            },
            Component {
                size: size_of::<Vector3>(),
                offset: offset_of!(CommonVertex, color),
            },
            Component {
                size: size_of::<Vector3>(),
                offset: offset_of!(CommonVertex, norm),
            },
            Component {
                size: size_of::<Vector2>(),
                offset: offset_of!(CommonVertex, uv),
            },
            Component {
                size: size_of::<Vector4>(),
                offset: offset_of!(CommonVertex, tan),
            },
        ];
        COMPONENTS
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Zeroable, Pod)]
pub struct SimpleVertex {
    pub(crate) pos: Vector3,
    pub(crate) color: Vector3,
    pub(crate) norm: Vector3,
}

impl Vertex for SimpleVertex {
    fn pos(&mut self) -> &mut Vector3 {
        &mut self.pos
    }

    fn components() -> &'static [Component] {
        const COMPONENTS: &[Component] = &[
            Component {
                size: size_of::<Vector3>(),
                offset: offset_of!(SimpleVertex, pos),
            },
            Component {
                size: size_of::<Vector3>(),
                offset: offset_of!(SimpleVertex, color),
            },
            Component {
                size: size_of::<Vector3>(),
                offset: offset_of!(SimpleVertex, norm),
            },
        ];
        COMPONENTS
    }
}

impl From<CommonVertex> for SimpleVertex {
    fn from(value: CommonVertex) -> Self {
        Self {
            pos: value.pos,
            color: value.color,
            norm: value.norm,
        }
    }
}

pub struct MeshBuilder<V: Vertex> {
    pub vertices: Vec<V>,
    pub indices: Vec<u32>,
}

pub struct Mesh<V: Vertex> {
    pub vertices: Box<[V]>,
    pub indices: Box<[u32]>,
}

impl<V: Vertex> MeshBuilder<V> {
    fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn convert<T: Vertex + From<V>>(self) -> MeshBuilder<T> {
        let Self { vertices, indices } = self;
        MeshBuilder {
            vertices: vertices.into_iter().map(T::from).collect(),
            indices,
        }
    }

    pub fn build(self) -> Mesh<V> {
        let Self { vertices, indices } = self;
        Mesh {
            vertices: vertices.into_boxed_slice(),
            indices: indices.into_boxed_slice(),
        }
    }

    fn extend(mut self, mut value: Self) -> Self {
        let index_offest = self.vertices.len() as u32;
        for index in &mut value.indices {
            *index += index_offest;
        }
        self.indices.extend(&value.indices);
        self.vertices.extend(&value.vertices);
        self
    }

    pub fn offset(mut self, offset: Vector3) -> Self {
        for vert in &mut self.vertices {
            *vert.pos() = *vert.pos() + offset;
        }
        self
    }
}

impl MeshBuilder<CommonVertex> {
    pub fn plane_subdivided(
        num_subdiv: usize,
        u: Vector3,
        v: Vector3,
        color: Vector3,
        scale_uvs: bool,
    ) -> Self {
        let normal = u.cross(v).norm();
        let u_length = if scale_uvs { u.length() } else { 1.0 };
        let v_length = if scale_uvs { v.length() } else { 1.0 };
        let num_edge_vertices = 2 + num_subdiv;
        let num_vertices = num_edge_vertices.pow(2);
        let vertices = (0..num_vertices)
            .map(|index| (index / num_edge_vertices, index % num_edge_vertices))
            .map(|(i, j)| {
                let u_scale = j as f32 / (num_edge_vertices - 1) as f32;
                let v_scale = i as f32 / (num_edge_vertices - 1) as f32;
                CommonVertex {
                    pos: u_scale * u + v_scale * v,
                    color,
                    norm: normal,
                    uv: if scale_uvs {
                        Vector2::new(u_scale * u_length, v_scale * v_length)
                    } else {
                        Vector2::new(u_scale, v_scale)
                    },
                    tan: Vector4::zero(),
                }
            })
            .collect();

        let num_edge_quads = 1 + num_subdiv;
        let num_quads = num_edge_quads.pow(2);
        let indices = (0..num_quads)
            .map(|index| (index / num_edge_quads, index % num_edge_quads))
            .flat_map(|(i, j)| {
                let vertex_index = (i * num_edge_vertices + j) as u32;
                let next_row_vertex_index = vertex_index + num_edge_vertices as u32;
                [
                    vertex_index,
                    vertex_index + 1,
                    next_row_vertex_index,
                    next_row_vertex_index + 1,
                    next_row_vertex_index,
                    vertex_index + 1,
                ]
            })
            .collect::<Vec<_>>();
        Self { vertices, indices }
    }

    fn box_subdivided(num_subdiv: usize, extent: Vector3, scale_uvs: bool) -> Self {
        const FACES: &[(Vector3, Vector3, Vector3, Vector3)] = &[
            (
                Vector3::new(0.5, 0.5, -0.5),
                Vector3::new(0.0, -1.0, 0.0),
                Vector3::new(-1.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
            ),
            (
                Vector3::new(-0.5, -0.5, -0.5),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(0.0, 1.0, 0.0),
            ),
            (
                Vector3::new(-0.5, 0.5, -0.5),
                Vector3::new(0.0, -1.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(0.0, 0.0, 1.0),
            ),
            (
                Vector3::new(-0.5, 0.5, 0.5),
                Vector3::new(0.0, -1.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
                Vector3::new(1.0, 0.0, 0.0),
            ),
            (
                Vector3::new(0.5, 0.5, -0.5),
                Vector3::new(-1.0, 0.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(0.0, 1.0, 0.0),
            ),
            (
                Vector3::new(0.5, -0.5, -0.5),
                Vector3::new(0.0, 1.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                Vector3::new(0.0, 0.0, 1.0),
            ),
        ];
        FACES
            .iter()
            .map(|&(offset, u, v, color)| {
                Self::plane_subdivided(
                    num_subdiv,
                    u.hadamard(extent),
                    v.hadamard(extent),
                    color,
                    scale_uvs,
                )
                .offset(offset.hadamard(extent))
            })
            .fold(Self::new(), |builder, face| builder.extend(face))
    }
}

impl<V: Vertex + From<CommonVertex>> From<shape::Cube> for Mesh<V> {
    fn from(value: shape::Cube) -> Self {
        MeshBuilder::box_subdivided(0, Vector3::new(value.side, value.side, value.side), true)
            .convert()
            .build()
    }
}

impl<V: Vertex + From<CommonVertex>> From<shape::Sphere> for Mesh<V> {
    fn from(value: shape::Sphere) -> Self {
        const UNIT_SPHERE_SUBDIV: usize = 4;
        let num_subdiv =
            ((value.diameter * UNIT_SPHERE_SUBDIV as f32) as usize).max(UNIT_SPHERE_SUBDIV);
        let mut mesh = MeshBuilder::box_subdivided(num_subdiv, Vector3::new(1.0, 1.0, 1.0), false);
        for vert in &mut mesh.vertices {
            let dir = vert.pos.norm();
            vert.norm = dir;
            vert.pos = 0.5 * value.diameter * dir;
            vert.uv = value.diameter * vert.uv;
        }
        mesh.convert().build()
    }
}

impl<V: Vertex + From<CommonVertex>> From<shape::Box> for Mesh<V> {
    fn from(value: shape::Box) -> Self {
        MeshBuilder::box_subdivided(
            0,
            Vector3::new(value.width, value.depth, value.height),
            true,
        )
        .convert()
        .build()
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VertexNone {}

impl Vertex for VertexNone {
    fn pos(&mut self) -> &mut Vector3 {
        unreachable!()
    }

    fn components() -> &'static [Component] {
        unreachable!()
    }
}

pub trait MeshTypeList: 'static {
    const LEN: usize;
    type Vertex: Vertex;
    type Next: MeshTypeList;
}

pub trait MeshCollection: MeshTypeList {
    fn get(&self) -> &[Mesh<Self::Vertex>];
    fn next(&self) -> &Self::Next;
}

impl<T: 'static> MeshTypeList for TypedNil<T> {
    const LEN: usize = 0;
    type Vertex = VertexNone;
    type Next = Self;
}

impl MeshCollection for Nil {
    fn get(&self) -> &[Mesh<Self::Vertex>] {
        &[]
    }

    fn next(&self) -> &Self::Next {
        self
    }
}

impl<V: Vertex, N: MeshTypeList> MeshTypeList for Cons<Vec<Mesh<V>>, N> {
    const LEN: usize = N::LEN + 1;
    type Vertex = V;
    type Next = N;
}

impl<V: Vertex, N: MeshTypeList> MeshCollection for Cons<Vec<Mesh<V>>, N> {
    fn get(&self) -> &[Mesh<Self::Vertex>] {
        &self.head
    }

    fn next(&self) -> &Self::Next {
        &self.tail
    }
}
pub struct Meshes<L: MeshTypeList> {
    list: L,
}

impl Default for Meshes<Nil> {
    fn default() -> Self {
        Self::new()
    }
}

impl Meshes<Nil> {
    pub fn new() -> Self {
        Self { list: Nil::new() }
    }
}

impl<L: MeshTypeList> Meshes<L> {
    pub fn push<V: Vertex>(self, meshes: Vec<Mesh<V>>) -> Meshes<Cons<Vec<Mesh<V>>, L>> {
        Meshes {
            list: Cons {
                head: meshes,
                tail: self.list,
            },
        }
    }
}

impl<L: MeshTypeList> Deref for Meshes<L> {
    type Target = L;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}
