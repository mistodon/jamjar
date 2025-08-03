use crate::draw::{GlyphRegion, PixelRegion, Region};

type Point = [f32; 2];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pivot(pub Point);

impl Pivot {
    pub const TL: Pivot = Pivot([0., 0.]);
    pub const TM: Pivot = Pivot([0.5, 0.]);
    pub const TR: Pivot = Pivot([1., 0.]);
    pub const ML: Pivot = Pivot([0., 0.5]);
    pub const MM: Pivot = Pivot([0.5, 0.5]);
    pub const MR: Pivot = Pivot([1., 0.5]);
    pub const BL: Pivot = Pivot([0., 1.]);
    pub const BM: Pivot = Pivot([0.5, 1.]);
    pub const BR: Pivot = Pivot([1., 1.]);
}

impl Default for Pivot {
    fn default() -> Self {
        Pivot::TL
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub pos: Point,
    pub pivot: Pivot,
}

impl Anchor {
    pub const fn zero() -> Anchor {
        Anchor {
            pos: [0., 0.],
            pivot: Pivot::TL,
        }
    }

    pub const fn x(&self) -> f32 {
        self.pos[0]
    }

    pub const fn y(&self) -> f32 {
        self.pos[1]
    }

    pub const fn move_to(&self, pos: Point) -> Anchor {
        Anchor {
            pos,
            pivot: self.pivot,
        }
    }

    pub const fn rel(&self, from_pivot: Pivot) -> Anchor {
        Anchor {
            pos: self.pos,
            pivot: from_pivot,
        }
    }

    pub const fn rel_tl(&self) -> Anchor {
        self.rel(Pivot::TL)
    }
    pub const fn rel_tm(&self) -> Anchor {
        self.rel(Pivot::TM)
    }
    pub const fn rel_tr(&self) -> Anchor {
        self.rel(Pivot::TR)
    }
    pub const fn rel_ml(&self) -> Anchor {
        self.rel(Pivot::ML)
    }
    pub const fn rel_mm(&self) -> Anchor {
        self.rel(Pivot::MM)
    }
    pub const fn rel_mr(&self) -> Anchor {
        self.rel(Pivot::MR)
    }
    pub const fn rel_bl(&self) -> Anchor {
        self.rel(Pivot::BL)
    }
    pub const fn rel_bm(&self) -> Anchor {
        self.rel(Pivot::BM)
    }
    pub const fn rel_br(&self) -> Anchor {
        self.rel(Pivot::BR)
    }

    pub fn frame(&self, size: [f32; 2]) -> Frame {
        let [w, h] = size;
        let [x, y] = self.pos;
        let [px, py] = self.pivot.0;
        Frame {
            tl: [x - w * px, y - h * py],
            size,
        }
    }

    pub fn offset(&self, offset: [f32; 2]) -> Anchor {
        let [ox, oy] = offset;
        let [x, y] = self.pos;
        Anchor {
            pos: [x + ox, y + oy],
            pivot: self.pivot,
        }
    }

    pub fn left(&self, amount: f32) -> Anchor {
        self.offset([-amount, 0.])
    }

    pub fn right(&self, amount: f32) -> Anchor {
        self.offset([amount, 0.])
    }

    pub fn up(&self, amount: f32) -> Anchor {
        self.offset([0., -amount])
    }

    pub fn down(&self, amount: f32) -> Anchor {
        self.offset([0., amount])
    }

    #[cfg(feature = "font")]
    pub fn cursor(self) -> crate::font::Cursor {
        self.into()
    }
}

impl From<[f32; 2]> for Anchor {
    fn from(pos: [f32; 2]) -> Anchor {
        Anchor {
            pos,
            pivot: Pivot::TL,
        }
    }
}

impl Into<[f32; 2]> for Anchor {
    fn into(self) -> [f32; 2] {
        self.pos
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Frame {
    tl: Point,
    size: [f32; 2],
}

impl Frame {
    pub const fn zero() -> Frame {
        Frame {
            tl: [0., 0.],
            size: [0., 0.],
        }
    }

    pub const fn new(tl: Point, size: [f32; 2]) -> Frame {
        Frame { tl, size }
    }

    pub fn between<P: Into<Point>, Q: Into<Point>>(tl: P, br: Q) -> Frame {
        let tl = tl.into();
        let br = br.into();
        let w = br[0] - tl[0];
        let h = br[1] - tl[1];
        Frame { tl, size: [w, h] }
    }

    pub fn contains_point(&self, point: Point) -> bool {
        let [x, y] = point;
        let [x0, y0] = self.tl;
        let [w, h] = self.size;
        x0 <= x && x <= (x0 + w) && y0 <= y && y <= (y0 + h)
    }

    pub fn anchor(&self, pivot: Pivot) -> Anchor {
        let [w, h] = self.size;
        let [x, y] = self.tl;
        let [px, py] = pivot.0;
        Anchor {
            pos: [x + w * px, y + h * py],
            pivot: Pivot::TL,
        }
    }

    pub const fn top_left(&self) -> Point {
        self.tl
    }

    pub const fn size(&self) -> [f32; 2] {
        self.size
    }

    pub fn set_size(&self, size: [f32; 2], pivot: Pivot) -> Frame {
        self.anchor(pivot).frame(size)
    }

    pub fn set_width(&self, width: f32, pivot: Pivot) -> Frame {
        self.set_size([width, self.size[1]], pivot)
    }

    pub fn set_height(&self, height: f32, pivot: Pivot) -> Frame {
        self.set_size([self.size[0], height], pivot)
    }

    pub fn scale(&self, scale: [f32; 2], pivot: Pivot) -> Frame {
        let [w, h] = self.size;
        let [x, y] = scale;
        self.set_size([w * x, h * y], pivot)
    }

    pub fn move_to<P: Into<Point>>(&self, pos: P, pivot: Pivot) -> Frame {
        let [x0, y0] = self.anchor(pivot).pos;
        let [x1, y1] = pos.into();
        self.offset([x1 - x0, y1 - y0])
    }

    pub fn move_between<P: Into<Point>, Q: Into<Point>>(
        &self,
        pos0: P,
        pos1: Q,
        pivot: Pivot,
    ) -> Frame {
        let [x0, y0] = pos0.into();
        let [x1, y1] = pos1.into();
        let dest = [x0 + (x1 - x0) / 2., y0 + (y1 - y0) / 2.];
        self.move_to(dest, pivot)
    }

    pub fn anchor_to(&self, anchor: Anchor) -> Frame {
        self.move_to(anchor.pos, anchor.pivot)
    }

    pub fn align_x_to(&self, x: f32, pivot: Pivot) -> Frame {
        let [x0, _] = self.anchor(pivot).pos;
        self.offset([x - x0, 0.])
    }

    pub fn align_y_to(&self, y: f32, pivot: Pivot) -> Frame {
        let [_, y0] = self.anchor(pivot).pos;
        self.offset([0., y - y0])
    }

    pub const fn tl(&self) -> Anchor {
        Anchor {
            pos: self.tl,
            pivot: Pivot::TL,
        }
    }
    pub fn tm(&self) -> Anchor {
        self.anchor(Pivot::TM)
    }
    pub fn tr(&self) -> Anchor {
        self.anchor(Pivot::TR)
    }
    pub fn ml(&self) -> Anchor {
        self.anchor(Pivot::ML)
    }
    pub fn mm(&self) -> Anchor {
        self.anchor(Pivot::MM)
    }
    pub fn mr(&self) -> Anchor {
        self.anchor(Pivot::MR)
    }
    pub fn bl(&self) -> Anchor {
        self.anchor(Pivot::BL)
    }
    pub fn bm(&self) -> Anchor {
        self.anchor(Pivot::BM)
    }
    pub fn br(&self) -> Anchor {
        self.anchor(Pivot::BR)
    }

    pub fn grow_rel(&self, amount: [f32; 2], pivot: Pivot) -> Frame {
        let [ax, ay] = amount;
        let [w, h] = self.size;
        let [x, y] = self.tl;
        let [px, py] = pivot.0;

        Frame {
            tl: [x - ax * px, y - ay * py],
            size: [w + ax, h + ay],
        }
    }
    pub fn shrink_rel(&self, amount: [f32; 2], pivot: Pivot) -> Frame {
        let [ax, ay] = amount;
        self.grow_rel([-ax, -ay], pivot)
    }

    pub fn grow(&self, amount: [f32; 2]) -> Frame {
        self.grow_rel(amount, Pivot::MM)
    }
    pub fn shrink(&self, amount: [f32; 2]) -> Frame {
        self.shrink_rel(amount, Pivot::MM)
    }

    pub fn outset_rel(&self, amount: f32, pivot: Pivot) -> Frame {
        self.grow_rel([amount * 2., amount * 2.], pivot)
    }
    pub fn inset_rel(&self, amount: f32, pivot: Pivot) -> Frame {
        self.shrink_rel([amount * 2., amount * 2.], pivot)
    }

    pub fn outset(&self, amount: f32) -> Frame {
        self.outset_rel(amount, Pivot::MM)
    }
    pub fn inset(&self, amount: f32) -> Frame {
        self.inset_rel(amount, Pivot::MM)
    }

    pub fn offset(&self, amount: [f32; 2]) -> Frame {
        let [ox, oy] = amount;
        let [x, y] = self.tl;
        Frame {
            tl: [x + ox, y + oy],
            size: self.size,
        }
    }
    pub fn left(&self, amount: f32) -> Frame {
        self.offset([-amount, 0.])
    }
    pub fn right(&self, amount: f32) -> Frame {
        self.offset([amount, 0.])
    }
    pub fn up(&self, amount: f32) -> Frame {
        self.offset([0., -amount])
    }
    pub fn down(&self, amount: f32) -> Frame {
        self.offset([0., amount])
    }
}

impl From<[f32; 2]> for Frame {
    fn from(tl: [f32; 2]) -> Frame {
        Frame { tl, size: [0., 0.] }
    }
}

impl Into<[f32; 2]> for Frame {
    fn into(self) -> [f32; 2] {
        self.tl
    }
}

#[cfg(feature = "font")]
use crate::font::Cursor;

#[cfg(feature = "font")]
impl From<Cursor> for Anchor {
    fn from(cur: Cursor) -> Anchor {
        Anchor {
            pos: cur.end(),
            pivot: Pivot::TL,
        }
    }
}

#[cfg(feature = "font")]
impl Into<Cursor> for Anchor {
    fn into(self) -> Cursor {
        self.pos.into()
    }
}

#[cfg(feature = "font")]
impl From<Cursor> for Frame {
    fn from(cur: Cursor) -> Frame {
        cur.frame()
    }
}

#[cfg(feature = "font")]
impl Into<Cursor> for Frame {
    fn into(self) -> Cursor {
        self.tl.into()
    }
}

impl From<PixelRegion> for Frame {
    fn from(r: PixelRegion) -> Self {
        let [x, y] = r.upper_left;
        let [w, h] = r.size();
        Frame {
            tl: [x as f32, y as f32],
            size: [w as f32, h as f32],
        }
    }
}

impl From<Region> for Frame {
    fn from(r: Region) -> Self {
        let [x, y] = r.pixels.0;
        let [w, h] = r.size();
        Frame {
            tl: [x as f32, y as f32],
            size: [w as f32, h as f32],
        }
    }
}

impl From<GlyphRegion> for Frame {
    fn from(r: GlyphRegion) -> Self {
        Frame {
            tl: r.pos,
            size: r.size,
        }
    }
}
