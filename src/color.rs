pub type Color = [f32; 4];

pub const TRANS: Color = [0., 0., 0., 0.];
pub const BLACK: Color = [0., 0., 0., 1.];
pub const WHITE: Color = [1., 1., 1., 1.];
pub const RED: Color = [1., 0., 0., 1.];
pub const GREEN: Color = [0., 1., 0., 1.];
pub const BLUE: Color = [0., 0., 1., 1.];
pub const CYAN: Color = [0., 1., 1., 1.];
pub const MAGENTA: Color = [1., 0., 1., 1.];
pub const YELLOW: Color = [1., 1., 0., 1.];

pub fn alpha(c: Color, alpha: f32) -> Color {
    let mut c = c;
    c[3] = alpha;
    c
}
