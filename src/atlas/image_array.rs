use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

use image::RgbaImage;
use texture_packer::{MultiTexturePacker, TexturePackerConfig};

use crate::{
    atlas::Atlas,
    draw::{PixelRegion, Region},
};

pub struct ImageArrayAtlas<'a, K>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
{
    to_add: Vec<(K::Owned, RgbaImage)>,
    to_remove: Vec<K::Owned>,
    packed: Vec<(K::Owned, RgbaImage)>,
    packer: MultiTexturePacker<'a, RgbaImage, K::Owned>,
    regions: HashMap<K::Owned, (usize, Region)>,
    page_limit: Option<usize>,
    texture_size: [u32; 2],
    modified: bool,
}

impl<'a, K> ImageArrayAtlas<'a, K>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
{
    pub fn new(texture_size: [u32; 2], page_limit: Option<usize>) -> Self {
        let config = TexturePackerConfig {
            max_width: texture_size[0],
            max_height: texture_size[1],
            allow_rotation: false,
            border_padding: 4,
            texture_padding: 4,
            texture_extrusion: 0,
            trim: false,
            texture_outlines: false,
        };

        ImageArrayAtlas {
            to_add: vec![],
            to_remove: vec![],
            packed: vec![],
            packer: MultiTexturePacker::new_skyline(config),
            regions: Default::default(),
            page_limit,
            texture_size,
            modified: false,
        }
    }

    pub fn insert(&mut self, key: K::Owned, image: RgbaImage) {
        self.to_add.push((key, image));
        self.modified = true;
    }

    pub fn remove(&mut self, key: &K) {
        self.to_remove.push(key.to_owned());
        self.modified = true;
    }

    pub fn compile_into(&mut self, dest: &mut [RgbaImage]) -> Vec<(usize, PixelRegion)> {
        self.modified = false;

        // TODO: Handle removals

        let [tw, th] = self.texture_size;
        let [tw, th] = [tw as f32, th as f32];
        for (key, image) in self.to_add.drain(..) {
            self.packed.push((key.clone(), image.clone()));
            self.packer.pack_own(key.clone(), image.clone()).unwrap();

            let pages = self.packer.get_pages();

            if let Some(page_limit) = self.page_limit {
                assert!(pages.len() <= page_limit);
            }

            for (i, page) in pages.iter().enumerate() {
                if let Some(frame) = page.get_frame(&key) {
                    let texture_packer::Rect { x, y, w, h } = frame.frame;
                    let region = Region {
                        pixels: ([x, y], [w, h]),
                        uv: (
                            [x as f32 / tw, y as f32 / th],
                            [w as f32 / tw, h as f32 / th],
                        ),
                    };
                    self.regions.insert(key.clone(), (i, region));

                    let dest = &mut dest[i];
                    use image::GenericImage;
                    dest.copy_from(&image, x, y).unwrap();
                }
            }
        }

        // TODO: Return actual modified range
        vec![]
    }

    pub fn fetch(&self, key: &K) -> (usize, &Region) {
        let (page, region) = &self.regions[key];
        (*page, region)
    }

    pub fn modified(&self) -> bool {
        self.modified
    }
}
