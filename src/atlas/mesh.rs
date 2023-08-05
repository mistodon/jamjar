use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;

use crate::mesh::{Mesh, MeshIndex, Submeshes};

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

pub struct SubmeshAtlas<M, I, V>
where
    M: PartialEq + Eq + Hash,
    I: PartialEq + Eq + Hash,
    V: Copy,
{
    pub vertices: Vec<V>,
    pub indices: Vec<u16>,
    free_vertex_ranges: Vec<Range<u32>>,
    free_index_ranges: Vec<Range<u32>>,
    vertex_ranges: HashMap<M, Range<u32>>,
    submeshes: HashMap<I, (M, Submeshes)>,
    modified: bool,
}

impl<M, I, V> SubmeshAtlas<M, I, V>
where
    M: PartialEq + Eq + Hash,
    I: PartialEq + Eq + Hash,
    V: Copy,
{
    pub fn new() -> Self {
        SubmeshAtlas {
            vertices: vec![],
            indices: vec![],
            free_vertex_ranges: vec![],
            free_index_ranges: vec![],
            vertex_ranges: HashMap::default(),
            submeshes: HashMap::default(),
            modified: false,
        }
    }

    pub fn insert_vertices(&mut self, key: M, mut vertices: Vec<V>) {
        self.modified = true;

        let claimed_range = claim_free_range(&mut self.free_vertex_ranges, vertices.len() as u32);

        let range = match claimed_range {
            Some(claimed_range) => {
                let copy_range = claimed_range.start as usize..claimed_range.end as usize;
                self.vertices[copy_range].copy_from_slice(&vertices);
                claimed_range
            }
            None => {
                let start = self.vertices.len() as u32;
                self.vertices.append(&mut vertices);
                let end = self.vertices.len() as u32;
                start..end
            }
        };

        self.vertex_ranges.insert(key, range.clone());
    }

    pub fn insert_submeshes(
        &mut self,
        vertices_key: M,
        key: I,
        mut indices: Vec<u16>,
        mut submeshes: Submeshes,
    ) {
        self.modified = true;

        let vertex_number_offset = self.vertex_ranges[&vertices_key].start as u16;

        for index in &mut indices {
            *index += vertex_number_offset;
        }

        let claimed_range = claim_free_range(&mut self.free_index_ranges, indices.len() as u32);
        let index_buffer_offset = match claimed_range {
            Some(claimed_range) => {
                let copy_range = claimed_range.start as usize..claimed_range.end as usize;
                self.indices[copy_range].copy_from_slice(&indices);
                claimed_range.start
            }
            None => {
                let offset = self.indices.len() as u32;
                self.indices.append(&mut indices);
                offset
            }
        };

        submeshes.offset(index_buffer_offset);

        self.submeshes.insert(key, (vertices_key, submeshes));
    }

    pub fn compile(&mut self) -> Option<MeshIndex> {
        if self.modified {
            // TODO: Return only modified range
            self.modified = false;
            Some(MeshIndex {
                vertex_range: 0..self.vertices.len() as u32,
                index_range: 0..self.indices.len() as u32,
            })
        } else {
            None
        }
    }

    pub fn fetch_submesh<Q>(&self, key: &Q, submesh_index: Option<usize>) -> Option<MeshIndex>
    where
        I: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.submeshes.get(key).map(|(mesh_key, submeshes)| {
            let vertex_range = self.vertex_ranges[&mesh_key].clone();

            match submesh_index {
                None => MeshIndex {
                    vertex_range,
                    index_range: submeshes.index_range.clone(),
                },
                Some(i) => MeshIndex {
                    vertex_range,
                    index_range: submeshes.submeshes[i].clone(),
                },
            }
        })
    }

    pub fn modified(&self) -> bool {
        self.modified
    }

    pub fn remove_mesh<Q>(&mut self, key: &Q)
    where
        M: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let vertex_range = self.vertex_ranges.remove(key);
        if let Some(vertex_range) = vertex_range {
            self.free_vertex_ranges.push(vertex_range);

            let mut to_remove = vec![];
            for (submesh_key, (mesh_key, submeshes)) in self.submeshes.iter() {
                if mesh_key.borrow() == key {
                    to_remove.push(submesh_key.clone());
                    self.free_index_ranges.push(submeshes.index_range.clone());
                }
            }
        }

        self.consolidate_free_ranges();
    }

    fn consolidate_free_ranges(&mut self) {
        consolidate_free_ranges(&mut self.free_vertex_ranges);
        consolidate_free_ranges(&mut self.free_index_ranges);
    }
}

fn consolidate_free_ranges(ranges: &mut Vec<Range<u32>>) {
    ranges.sort_by_key(|r| r.start);

    if ranges.len() < 2 {
        return;
    }

    let mut after = ranges.len() - 1;
    while after > 0 {
        let before = after - 1;

        let after_range = ranges[after].clone();
        let before_range = &mut ranges[before];

        if before_range.end >= after_range.start {
            before_range.end = after_range.end;
            ranges.remove(after);
        }
        after -= 1;
    }
}

fn claim_free_range(ranges: &mut Vec<Range<u32>>, size: u32) -> Option<Range<u32>> {
    for i in 0..ranges.len() {
        let range = ranges[i].clone();
        let range_size = range.end - range.start;
        if range_size >= size {
            let claimed_end = range.start + size;
            let claimed = range.start..claimed_end;
            if range_size == size {
                ranges.remove(i);
            } else {
                let remainder = claimed_end..range.end;
                ranges[i] = remainder;
            }

            return Some(claimed);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consolidate_free_ranges() {
        let examples = &mut [
            vec![],
            vec![0..10],
            vec![20..30, 0..10],
            vec![0..10, 5..20],
            vec![0..10, 0..10, 0..10],
            vec![60..70, 5..10, 10..15, 15..20, 17..25, 20..30, 40..50],
        ];
        let expected = &[
            vec![],
            vec![0..10],
            vec![0..10, 20..30],
            vec![0..20],
            vec![0..10],
            vec![5..30, 40..50, 60..70],
        ];

        for example in examples.iter_mut() {
            consolidate_free_ranges(example);
        }

        for (example, expected) in examples.iter().zip(expected.iter()) {
            assert_eq!(example, expected);
        }
    }

    #[test]
    fn test_claim_free_range() {
        let examples = &mut [
            (0, None, vec![]),
            (10, Some(0..10), vec![0..10]),
            (5, Some(0..5), vec![0..10]),
            (20, None, vec![0..10]),
        ];
        let expected = &[vec![], vec![], vec![5..10], vec![0..10]];

        for (size, return_value, example) in examples.iter_mut() {
            assert_eq!(claim_free_range(example, *size), *return_value);
        }

        for ((_, _, example), expected) in examples.iter().zip(expected.iter()) {
            assert_eq!(example, expected);
        }
    }
}
