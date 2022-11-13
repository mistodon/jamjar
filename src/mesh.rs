use std::ops::Range;

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

#[cfg(feature = "gltf")]
mod gltf {
    use crate::math::*;

    pub fn load_obj_or_whatever(
        obj_file: &[u8],
    ) -> gltf::Result<crate::mesh::Mesh<crate::draw::popup::Vertex>> {
        let (doc, buffers, _images) = gltf::import_slice(obj_file)?;
        let mesh_primitives = doc.meshes().next().unwrap().primitives().next().unwrap();

        let positions = attribute_view::<Vec3<f32>>(
            &mesh_primitives.get(&gltf::Semantic::Positions).unwrap(),
            &buffers,
        );
        let normals = attribute_view::<Vec3<f32>>(
            &mesh_primitives.get(&gltf::Semantic::Normals).unwrap(),
            &buffers,
        );
        let uvs = attribute_view::<Vec2<f32>>(
            &mesh_primitives.get(&gltf::Semantic::TexCoords(0)).unwrap(),
            &buffers,
        );
        let colors = attribute_view::<Vec4<u16>>(
            &mesh_primitives.get(&gltf::Semantic::Colors(0)).unwrap(),
            &buffers,
        );

        let indices = attribute_view::<u16>(&mesh_primitives.indices().unwrap(), &buffers).to_vec();

        let flip_z = vec3(1., 1., -1.);
        let vertices: Vec<crate::draw::popup::Vertex> = (0..positions.len())
            .into_iter()
            .map(|i| crate::draw::popup::Vertex {
                position: (positions[i] * flip_z).extend(1.).0,
                normal: (normals[i] * flip_z).extend(0.).0,
                uv: uvs[i].extend(0.).extend(0.).0,
                color: (colors[i].as_f32() / 65535.).0,
            })
            .collect();

        Ok(crate::mesh::Mesh { vertices, indices })
    }

    fn attribute_view<'a, T>(
        accessor: &gltf::Accessor<'a>,
        buffers: &[gltf::buffer::Data],
    ) -> &'a [T] {
        let view = accessor.view().expect("Cannot handle sparse attributes");
        let expected_length = accessor.size() * accessor.count();
        let buffer = &buffers[view.buffer().index()];
        let bytes = &buffer[view.offset()..(view.offset() + view.length())];

        assert!(std::mem::size_of::<T>() * accessor.count() == expected_length);
        assert!(bytes.len() == expected_length);

        unsafe {
            std::slice::from_raw_parts(bytes.as_ptr() as _, bytes.len() / std::mem::size_of::<T>())
        }
    }
}

#[cfg(feature = "gltf")]
pub use self::gltf::*;
