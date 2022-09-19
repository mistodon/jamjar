use std::collections::HashMap;
use std::hash::Hash;

use crate::{
    atlas::Atlas,
    mesh::{Mesh, MeshIndex},
};

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
}

impl<K, V> Atlas<(K::Owned, Mesh<V>), K, MeshIndex, Mesh<V>, MeshIndex> for MeshAtlas<K, V>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
    V: Copy,
{
    fn insert(&mut self, insertion: (K::Owned, Mesh<V>)) {
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

    fn remove_and_invalidate(&mut self, key: &K) {
        let mut source_meshes = std::mem::replace(&mut self.source_meshes, Default::default());
        self.backing.clear();
        self.indices.clear();
        for (key, mesh) in source_meshes {
            self.insert((key, mesh));
        }
    }

    fn fetch(&self, key: &K) -> MeshIndex {
        self.indices[key].clone()
    }

    fn compile_into(&mut self, dest: &mut Mesh<V>) -> Option<MeshIndex> {
        if self.modified {
            self.modified = false;
            *dest = self.backing.clone();
            Some(self.backing.span_index())
        } else {
            None
        }
    }

    fn modified(&self) -> bool {
        self.modified
    }
}
