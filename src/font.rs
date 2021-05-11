use std::sync::atomic::{AtomicUsize, Ordering};

use rusttype::{Font as RTFont, PositionedGlyph};

static mut FONT_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub struct Glyph {
    pub(crate) font_id: usize,
    pub(crate) glyph: PositionedGlyph<'static>,
}

pub struct Font {
    font_id: usize,
    font: RTFont<'static>,
}

impl Font {
    pub fn new(bytes: Vec<u8>) -> Self {
        let font_id = unsafe { FONT_COUNT.fetch_add(1, Ordering::Relaxed) };
        let font = RTFont::try_from_vec(bytes).unwrap();

        Font { font_id, font }
    }

    pub fn test_glyph(&self, c: char, pos: [f32; 2]) -> Glyph {
        use rusttype::{Point, Scale};

        let g = self.font.glyph(c);
        let g = g.scaled(Scale { x: 11., y: 11. });

        let [x, y] = pos;
        let g = g.positioned(Point { x, y });

        Glyph {
            font_id: self.font_id,
            glyph: g,
        }
    }
}
