use std::hash::Hash;

use image::RgbaImage;
use rusttype::gpu_cache::Cache;

use crate::{atlas::ImageAtlas, draw::GlyphRegion, font::Glyph};

pub struct FontImageAtlas<'a, K: Clone + Hash + Eq> {
    pub images: ImageAtlas<'a, K>,
    pub fonts: FontAtlas,
    backing_image: RgbaImage,
}

impl<'a, K: Clone + Hash + Eq> FontImageAtlas<'a, K> {
    pub fn new(size: [u32; 2], split_at: u32) -> Self {
        assert!(split_at < size[0]);

        let height = size[1];
        let other_width = size[0] - split_at;

        FontImageAtlas {
            images: ImageAtlas::with_area_in_size(([split_at, 0], [other_width, height]), size),
            fonts: FontAtlas::with_area_in_size(([0, 0], [split_at, height]), size),
            backing_image: RgbaImage::new(size[0], size[1]),
        }
    }

    pub fn compile_if_modified(&mut self) -> bool {
        let mut updated = false;

        if self.images.modified() {
            updated |= self.images.compile_into(&mut self.backing_image);
        }

        if self.fonts.modified() {
            updated |= self.fonts.compile_into(&mut self.backing_image);
        }

        updated
    }

    pub fn image(&self) -> &RgbaImage {
        &self.backing_image
    }
}

pub struct FontAtlas {
    glyph_cache: Cache<'static>,
    backing_image_size: [u32; 2],
    available_area: ([u32; 2], [u32; 2]),
    modified: bool,
}

impl FontAtlas {
    pub fn new() -> Self {
        Self::with_size([4096, 4096])
    }

    pub fn with_size(backing_size: [u32; 2]) -> Self {
        Self::with_area_in_size(([0, 0], backing_size), backing_size)
    }

    pub fn with_area_in_size(
        (topleft, size): ([u32; 2], [u32; 2]),
        backing_size: [u32; 2],
    ) -> Self {
        FontAtlas {
            glyph_cache: Cache::builder()
                .dimensions(size[0], size[1])
                .position_tolerance(0.1)
                .scale_tolerance(0.1)
                .pad_glyphs(true)
                .multithread(true)
                .build(),
            backing_image_size: backing_size,
            available_area: (topleft, size),
            modified: true,
        }
    }

    pub fn insert(&mut self, glyph: &Glyph) {
        self.glyph_cache
            .queue_glyph(glyph.font_id, glyph.glyph.clone());
    }

    pub fn region(&self, glyph: &Glyph) -> Option<GlyphRegion> {
        let [bw, bh] = self.backing_image_size;
        let ([ax, ay], [aw, ah]) = self.available_area;
        let scale_u = aw as f32 / bw as f32;
        let scale_v = ah as f32 / bh as f32;
        let off_u = ax as f32 / bw as f32;
        let off_v = ay as f32 / bh as f32;

        let scale = glyph.glyph.scale();
        let ascent = glyph.glyph.font().v_metrics(scale).ascent;

        let coords = self
            .glyph_cache
            .rect_for(glyph.font_id, &glyph.glyph)
            .unwrap();

        coords.map(|(uv_rect, px_rect)| {
            use rusttype::Point;

            let Point { x, y } = px_rect.min;
            let w = px_rect.width() as f32;
            let h = px_rect.height() as f32;

            let Point { x: u, y: v } = uv_rect.min;
            let uw = uv_rect.width();
            let vh = uv_rect.height();
            let uv = (
                [u * scale_u + off_u, v * scale_v + off_v],
                [uw * scale_u, vh * scale_v],
            );

            GlyphRegion {
                pos: [x as f32, y as f32 + ascent],
                size: [w, h],
                uv,
            }
        })
    }

    pub fn compile(&mut self) -> RgbaImage {
        let [bw, bh] = self.backing_image_size;
        let mut atlas = RgbaImage::new(bw, bh);
        self.compile_into(&mut atlas);
        atlas
    }

    pub fn compile_into(&mut self, atlas: &mut RgbaImage) -> bool {
        let mut upload_required = false;

        let ([ax, ay], _) = self.available_area;

        self.glyph_cache
            .cache_queued(|dest_rect, data| {
                use rusttype::Point;

                let Point { x, y } = dest_rect.min;
                let w = dest_rect.width();
                let h = dest_rect.height();
                for dy in 0..h {
                    for dx in 0..w {
                        let alpha = data[(dy * w + dx) as usize];
                        atlas.put_pixel(x + ax + dx, y + ay + dy, [255, 255, 255, alpha].into());
                    }
                }

                upload_required = true;
            })
            .unwrap();

        self.modified = true;
        upload_required
    }

    pub fn modified(&self) -> bool {
        self.modified
    }
}
