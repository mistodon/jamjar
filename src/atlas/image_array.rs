use std::collections::HashMap;
use std::hash::Hash;

use image::RgbaImage;
use texture_packer::{MultiTexturePacker, TexturePackerConfig};

use crate::draw::{PixelRegion, Region};

pub struct ImageArrayAtlas<'a, K>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
{
    to_compile: Vec<K::Owned>,
    packed: HashMap<K::Owned, RgbaImage>,
    packer: MultiTexturePacker<'a, RgbaImage, K::Owned>,
    regions: HashMap<K::Owned, (usize, Region)>,
    page_limit: Option<usize>,
    texture_size: [u32; 2],
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
            border_padding: 2,
            texture_padding: 2,
            texture_extrusion: 2,
            trim: false,
            texture_outlines: false,
        };

        ImageArrayAtlas {
            to_compile: vec![],
            packed: HashMap::new(),
            packer: MultiTexturePacker::new_skyline(config),
            regions: Default::default(),
            page_limit,
            texture_size,
        }
    }

    pub fn insert(&mut self, key: K::Owned, image: RgbaImage) {
        let [tw, th] = self.texture_size;
        let [tw, th] = [tw as f32, th as f32];

        self.packed.insert(key.clone(), image.clone());
        self.to_compile.push(key.clone());
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
            }
        }
    }

    pub fn compile_into(&mut self, dest: &mut [RgbaImage]) -> Vec<(usize, PixelRegion)> {
        use std::borrow::Borrow;

        let pages = self.packer.get_pages();
        for (i, page) in pages.iter().enumerate() {
            let frames = page.get_frames();
            for key in &self.to_compile {
                let texture_packer::Rect { x, y, .. } = frames[key.borrow()].frame;
                let image = &self.packed[key.borrow()];
                let dest = &mut dest[i];
                use image::GenericImage;
                dest.copy_from(image, x, y).unwrap();
            }
        }

        self.to_compile.clear();

        // TODO: Return actual modified range
        vec![]
    }

    pub fn fetch(&self, key: &K) -> Option<(usize, &Region)> {
        self.regions.get(key).map(|(a, b)| (*a, b))
    }

    pub fn modified(&self) -> bool {
        !self.to_compile.is_empty()
    }
}
