use std::hash::Hash;

use image::RgbaImage;

use crate::atlas::{font::FontAtlas, image::ImageAtlas, Atlas};

pub struct FontImageAtlas<'a, K>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
{
    pub images: ImageAtlas<'a, K>,
    pub fonts: FontAtlas,
    backing_image_size: [u32; 2],
}

impl<'a, K> FontImageAtlas<'a, K>
where
    K: ToOwned + Eq + Hash + ?Sized,
    K::Owned: Clone + Eq + Hash,
{
    pub fn new(size: [u32; 2], split_at: u32) -> Self {
        assert!(split_at < size[0]);

        let height = size[1];
        let other_width = size[0] - split_at;

        FontImageAtlas {
            images: ImageAtlas::with_area_in_size(([split_at, 0], [other_width, height]), size),
            fonts: FontAtlas::with_area_in_size(([0, 0], [split_at, height]), size),
            backing_image_size: size,
        }
    }

    pub fn compile_into(&mut self, dest: &mut RgbaImage) -> bool {
        let mut updated = false;

        if self.images.modified() {
            updated |= self.images.compile_into(dest).is_some();
        }

        if self.fonts.modified() {
            updated |= self.fonts.compile_into(dest).is_some();
        }

        updated
    }

    pub fn compile(&mut self) -> RgbaImage {
        let [bw, bh] = self.backing_image_size;
        let mut atlas = RgbaImage::new(bw, bh);
        self.compile_into(&mut atlas);
        atlas
    }

    pub fn modified(&self) -> bool {
        self.images.modified() || self.fonts.modified()
    }
}
