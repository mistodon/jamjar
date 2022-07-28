use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;

use crate::{
    atlas::Atlas,
    mesh::{MeshIndex, Mesh},
};

pub struct MeshAtlas<K, V>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
    V: Copy,
{
    backing: Mesh<V>,
    indices: HashMap<K::Owned, MeshIndex>,
    modified: bool,
}

impl<K, V> Atlas<(K::Owned, Mesh<V>), K, MeshIndex, Mesh<V>, MeshIndex> for MeshAtlas<K, V>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
    V: Copy,
{
    fn insert(&mut self, insertion: (K::Owned, Mesh<V>)) {
        let (key, mesh) = insertion;

        let v_len = mesh.vertex_count();
        let i_len = mesh.index_count();
        let v_start = self.backing.vertex_count();
        let i_start = self.backing.index_count();

        let index = MeshIndex {
            vertex_range: v_start .. (v_start + v_len),
            index_range: i_start .. (i_start + i_len),
        };

        self.backing.push(mesh);
        self.indices.insert(key, index);

        self.modified = true;
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
