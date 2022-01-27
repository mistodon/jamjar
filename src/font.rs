use std::sync::atomic::{AtomicUsize, Ordering};

use rusttype::{Font as RTFont, GlyphId, PositionedGlyph};

use crate::layout::Frame;

static mut FONT_COUNT: AtomicUsize = AtomicUsize::new(0);

type FontId = usize;
type LineHeight = f32;

#[derive(Debug, Clone)]
pub struct Glyph {
    pub(crate) font_id: usize,
    pub(crate) glyph: PositionedGlyph<'static>,
}

pub struct Font {
    font_id: usize,
    font: RTFont<'static>,
    pub default_size: f32,
}

#[derive(Debug, Clone)]
pub struct Cursor {
    pos: [f32; 2],
    original_start_pos: [f32; 2],
    prev_glyph: Option<(FontId, GlyphId, LineHeight)>,
}

impl Cursor {
    pub const fn pos(&self) -> [f32; 2] {
        self.pos
    }

    pub const fn original_start_pos(&self) -> [f32; 2] {
        self.original_start_pos
    }

    pub fn end(&self) -> [f32; 2] {
        let line_height = self.prev_glyph.map(|x| x.2).unwrap_or(0.);
        [self.pos[0], self.pos[1] + line_height]
    }

    pub fn span_from<P: Into<[f32; 2]>>(&self, from: P) -> [f32; 2] {
        let [x0, y0] = from.into();
        let [x1, y1] = self.end();
        [x1 - x0, y1 - y0]
    }

    pub fn span(&self) -> [f32; 2] {
        self.span_from(self.original_start_pos())
    }

    pub fn frame(&self) -> Frame {
        let tl = self.original_start_pos();
        let size = self.span();
        Frame::new(tl, size)
    }
}

impl Into<[f32; 2]> for Cursor {
    fn into(self) -> [f32; 2] {
        self.pos
    }
}

impl From<[f32; 2]> for Cursor {
    fn from(pos: [f32; 2]) -> Self {
        Cursor {
            pos,
            original_start_pos: pos,
            prev_glyph: None,
        }
    }
}

impl Font {
    pub fn new(bytes: Vec<u8>, default_size: f32) -> Self {
        let font_id = unsafe { FONT_COUNT.fetch_add(1, Ordering::Relaxed) };
        let font = RTFont::try_from_vec(bytes).unwrap();

        Font {
            font_id,
            font,
            default_size,
        }
    }

    pub fn glyph(&self, ch: char, pos: [f32; 2], size: f32, scale_factor: f64) -> Glyph {
        use rusttype::{Point, Scale};

        let sf = scale_factor as f32;
        let scale = Scale {
            x: size * sf,
            y: size * sf,
        };
        let start = Point {
            x: pos[0] * sf,
            y: pos[1] * sf,
        };
        let g = self.font.glyph(ch);
        let g = g.scaled(scale);
        let g = g.positioned(start);

        Glyph {
            font_id: self.font_id,
            glyph: g,
        }
    }

    pub fn layout_line<S: AsRef<str>>(
        &self,
        text: S,
        start: [f32; 2],
        size: f32,
        scale_factor: f64,
    ) -> Vec<Glyph> {
        let (_cur, glyphs) = self.layout_line_cur(text, start, size, scale_factor);
        glyphs
    }

    pub fn layout_line_cur<S: AsRef<str>, P: Into<Cursor>>(
        &self,
        text: S,
        start: P,
        size: f32,
        scale_factor: f64,
    ) -> (Cursor, Vec<Glyph>) {
        use rusttype::{Point, Scale};

        let sf = scale_factor as f32;
        let scale = Scale {
            x: size * sf,
            y: size * sf,
        };

        let mut cursor = start.into();

        let line_height = {
            let metrics = self.font.v_metrics(scale);
            (metrics.ascent - metrics.descent) + metrics.line_gap
        };

        let glyphs = text
            .as_ref()
            .chars()
            .map(|ch| {
                let g = self.font.glyph(ch);
                let g = g.scaled(scale);
                if let Some((prev_id, last, _)) = cursor.prev_glyph {
                    if prev_id == self.font_id {
                        cursor.pos[0] += self.font.pair_kerning(scale, last, g.id());
                    }
                }
                let w = g.h_metrics().advance_width;
                let next = g.positioned(Point {
                    x: cursor.pos[0] * sf,
                    y: cursor.pos[1] * sf,
                });
                cursor.prev_glyph = Some((self.font_id, next.id(), line_height / sf));
                cursor.pos[0] += w / sf;
                Glyph {
                    font_id: self.font_id,
                    glyph: next,
                }
            })
            .collect();

        (cursor, glyphs)
    }
}
