use std::collections::HashMap;
use std::hash::Hash;

use crate::mesh::{Mesh, MeshIndex};

pub struct MeshAtlas<K, V>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
    V: Copy,
{
    backing: Mesh<V>,
    indices: HashMap<K::Owned, MeshIndex>,
    source_meshes: HashMap<K::Owned, Mesh<V>>,
    modified: bool,
}

impl<K, V> MeshAtlas<K, V>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
    V: Copy,
{
    pub fn new() -> Self {
        MeshAtlas {
            backing: Mesh::new(),
            indices: HashMap::new(),
            source_meshes: HashMap::new(),
            modified: false,
        }
    }

    pub fn insert(&mut self, insertion: (K::Owned, Mesh<V>)) {
        let (key, mesh) = insertion;

        let v_len = mesh.vertex_count() as u32;
        let i_len = mesh.index_count() as u32;
        let v_start = self.backing.vertex_count() as u32;
        let i_start = self.backing.index_count() as u32;

        let index = MeshIndex {
            vertex_range: v_start..(v_start + v_len),
            index_range: i_start..(i_start + i_len),
        };

        self.backing.push(mesh.clone());
        self.indices.insert(key.clone(), index);
        self.source_meshes.insert(key, mesh);

        self.modified = true;
    }

    pub fn compile_into(&mut self, dest: &mut Mesh<V>) -> Option<MeshIndex> {
        if self.modified {
            self.modified = false;
            *dest = self.backing.clone();
            Some(self.backing.span_index())
        } else {
            None
        }
    }

    pub fn fetch(&self, key: &K) -> Option<MeshIndex> {
        self.indices.get(key).cloned()
    }

    pub fn modified(&self) -> bool {
        self.modified
    }
}
