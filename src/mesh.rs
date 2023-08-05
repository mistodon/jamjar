use std::collections::HashMap;
use std::ops::Range;

use okmath::*;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 4],
    pub normal: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct Submeshes {
    pub index_range: Range<u32>,
    pub submeshes: Vec<Range<u32>>,
}

impl Submeshes {
    pub fn offset(&mut self, offset: u32) {
        self.index_range = (self.index_range.start + offset)..(self.index_range.end + offset);
        for submesh in &mut self.submeshes {
            *submesh = (submesh.start + offset)..(submesh.end + offset);
        }
    }
}

#[derive(Debug, Clone)]
pub struct MeshIndex {
    pub vertex_range: Range<u32>,
    pub index_range: Range<u32>,
}

#[derive(Debug, Clone)]
pub struct Mesh<V> {
    pub vertices: Vec<V>,
    pub indices: Vec<u16>,
}

impl<V> Mesh<V> {
    pub fn new() -> Self {
        Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn offset_indices(&mut self, amount: u16) {
        for i in self.indices.iter_mut() {
            *i += amount;
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    pub fn push(&mut self, mesh: Mesh<V>) {
        let mut mesh = mesh;
        mesh.offset_indices(self.vertex_count() as u16);

        self.vertices.append(&mut mesh.vertices);
        self.indices.append(&mut mesh.indices);
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    pub fn span_index(&self) -> MeshIndex {
        MeshIndex {
            vertex_range: 0..self.vertex_count() as u32,
            index_range: 0..self.index_count() as u32,
        }
    }
}

/// Stitches coincident edge pairs together.
///
/// Appends any new vertices generated to `vertices`
/// and returns a new list of indices for the stitched version
/// of the mesh.
pub fn stitch_mesh(vertices: &mut Vec<Vertex>, indices: &[u16]) -> Vec<u16> {
    let resolution = 10000.;
    let mut seen_map: HashMap<Vec3<isize>, Vec<u16>> = HashMap::new();
    let mut replace_map: HashMap<u16, u16> = HashMap::new();

    for i in 0..vertices.len() {
        let vertex = &vertices[i];
        let pos = (Vec4::new(vertex.position).retract() * resolution).as_isize();
        let bucket = seen_map.entry(pos).or_insert_with(|| vec![]);
        bucket.push(i as u16);
    }

    for points in seen_map.values() {
        if points.len() > 1 {
            let mut normal = vec4(0., 0., 0., 0.);
            for &p in points {
                normal += Vec4::new(vertices[p as usize].normal);
            }

            let new_index = vertices.len() as u16;
            let mut new_vertex = vertices[points[0] as usize].clone();
            new_vertex.normal = normal.norm().0;
            vertices.push(new_vertex);
            for &p in points {
                replace_map.insert(p, new_index);
            }
        }
    }

    let new_indices = indices
        .iter()
        .map(|i| *replace_map.get(i).unwrap_or(i))
        .collect::<Vec<_>>();

    new_indices
}

#[cfg(feature = "gltf")]
mod gltf {
    use std::borrow::Cow;

    use crate::math::*;

    use super::Vertex;

    pub fn load_glb(obj_file: &[u8]) -> gltf::Result<crate::mesh::Mesh<Vertex>> {
        let (doc, buffers, _images) = gltf::import_slice(obj_file)?;
        let mesh_primitives = doc.meshes().next().unwrap().primitives().next().unwrap();

        let positions = attribute_view::<Vec3<f32>>(
            0,
            &mesh_primitives.get(&gltf::Semantic::Positions),
            &buffers,
        );
        let normals = attribute_view::<Vec3<f32>>(
            positions.len(),
            &mesh_primitives.get(&gltf::Semantic::Normals),
            &buffers,
        );
        let uvs = attribute_view::<Vec2<f32>>(
            positions.len(),
            &mesh_primitives.get(&gltf::Semantic::TexCoords(0)),
            &buffers,
        );
        let colors = attribute_view::<Vec4<u16>>(
            positions.len(),
            &mesh_primitives.get(&gltf::Semantic::Colors(0)),
            &buffers,
        );

        let indices = attribute_view::<u16>(0, &mesh_primitives.indices(), &buffers).to_vec();

        let flip_z = vec3(1., 1., -1.);
        let mut vertices: Vec<Vertex> = (0..positions.len())
            .into_iter()
            .map(|i| Vertex {
                position: (positions[i] * flip_z).extend(1.).0,
                normal: (normals[i] * flip_z).extend(0.).0,
                uv: uvs[i].extend(0.).extend(0.).0,
                color: (colors[i].as_f32() / 65535.).0,
            })
            .collect();

        // Hack to avoid annoying missing vertex colors
        for vertex in &mut vertices {
            if vertex.color == [0., 0., 0., 0.] {
                vertex.color = [1., 1., 1., 1.];
            }
        }

        Ok(crate::mesh::Mesh { vertices, indices })
    }

    fn attribute_view<'a, T: Default + Clone>(
        fallback_length: usize,
        accessor: &Option<gltf::Accessor<'a>>,
        buffers: &[gltf::buffer::Data],
    ) -> Cow<'a, [T]> {
        match accessor {
            None => Cow::Owned(vec![T::default(); fallback_length]),
            Some(accessor) => {
                let view = accessor.view().expect("Cannot handle sparse attributes");
                let expected_length = accessor.size() * accessor.count();
                let buffer = &buffers[view.buffer().index()];
                let bytes = &buffer[view.offset()..(view.offset() + view.length())];

                assert!(std::mem::size_of::<T>() * accessor.count() == expected_length);
                assert!(bytes.len() == expected_length);

                Cow::Borrowed(unsafe {
                    std::slice::from_raw_parts(
                        bytes.as_ptr() as _,
                        bytes.len() / std::mem::size_of::<T>(),
                    )
                })
            }
        }
    }
}

#[cfg(feature = "gltf")]
pub use self::gltf::*;
