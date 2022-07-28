use std::ops::Range;

#[derive(Debug, Clone)]
pub struct MeshIndex {
    pub vertex_range: Range<usize>,
    pub index_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Mesh<V> {
    pub vertices: Vec<V>,
    pub indices: Vec<u16>,
}

impl<V> Mesh<V> {
    pub fn offset_indices(&mut self, amount: u16) {
        for i in self.indices.iter_mut() {
            *i += amount;
        }
    }

    pub fn push(&mut self, mesh: Mesh<V>) {
        let mut mesh = mesh;
        mesh.offset_indices(self.index_count() as u16);

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
            vertex_range: 0..self.vertex_count(),
            index_range: 0..self.index_count(),
        }
    }
}
