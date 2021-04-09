use std::collections::HashMap;
use std::hash::Hash;

use image::RgbaImage;
use texture_packer::{TexturePacker, TexturePackerConfig};

use crate::draw::Region;

const MAX_SIZE: u32 = 4096;
const MAX_SIZE_F: f32 = 4096.;

pub struct ImageAtlas<'a, K: Clone + Hash + Eq> {
    regions: HashMap<K, Region>,
    source_images: HashMap<K, RgbaImage>,
    packer: TexturePacker<'a, RgbaImage>,
    pre_made_atlas: Option<RgbaImage>,
}

impl<'a, K: Clone + Hash + Eq> ImageAtlas<'a, K> {
    fn config() -> TexturePackerConfig {
        TexturePackerConfig {
            max_width: MAX_SIZE,
            max_height: MAX_SIZE,
            allow_rotation: false,
            border_padding: 2,
            texture_padding: 2,
            trim: false,
            ..Default::default()
        }
    }

    pub fn new() -> Self {
        ImageAtlas {
            regions: Default::default(),
            source_images: Default::default(),
            packer: TexturePacker::new_skyline(Self::config()),
            pre_made_atlas: None,
        }
    }

    pub fn pre_made(atlas_image: RgbaImage, regions: HashMap<K, Region>) -> Self {
        let mut packer = TexturePacker::new_skyline(Self::config());
        packer.pack_own(String::new(), atlas_image.clone()).unwrap();
        let frame = packer.get_frame("").unwrap().frame;
        assert!(
            frame.x == 0 && frame.y == 1,
            "Oops, that's not how I thought this worked"
        );

        ImageAtlas {
            regions,
            source_images: Default::default(),
            packer,
            pre_made_atlas: Some(atlas_image),
        }
    }

    pub fn insert(&mut self, key: K, image: RgbaImage) {
        let string_key = self.source_images.len().to_string();
        self.packer
            .pack_own(string_key.clone(), image.clone())
            .unwrap();
        let frame = self.packer.get_frame(&string_key).unwrap().frame;

        let region = Region {
            pixels: ([frame.x, frame.y], [frame.w, frame.h]),
            uv: (
                [frame.x as f32 / MAX_SIZE_F, frame.y as f32 / MAX_SIZE_F],
                [frame.w as f32 / MAX_SIZE_F, frame.h as f32 / MAX_SIZE_F],
            ),
        };

        self.regions.insert(key.clone(), region);
        self.source_images.insert(key, image);
    }

    pub fn region<Q>(&self, key: &Q) -> Region
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.regions[key]
    }

    pub fn compile(&self) -> RgbaImage {
        use image::GenericImage;

        let mut atlas = RgbaImage::new(MAX_SIZE, MAX_SIZE);
        if let Some(pre_made_atlas) = &self.pre_made_atlas {
            atlas.copy_from(pre_made_atlas, 0, 0).unwrap();
        }

        for (key, region) in self.regions.iter() {
            let image = self.source_images.get(&key);

            // If there's no image, this region must be from the pre-made atlas
            if let Some(image) = image {
                atlas
                    .copy_from(image, region.pixels.0[0], region.pixels.0[1])
                    .unwrap();
            }
        }

        atlas
    }
}
