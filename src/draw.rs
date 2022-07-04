#[cfg(feature = "draw_forever")]
pub mod forever;

#[cfg(feature = "draw_groove")]
pub mod groove;

#[cfg(feature = "draw_sloth")]
pub mod sloth;

pub mod backend {
    #[cfg(feature = "dx12")]
    pub type Dx12 = gfx_backend_dx12::Backend;
    #[cfg(feature = "opengl")]
    pub type OpenGL = gfx_backend_gl::Backend;
    #[cfg(feature = "metal")]
    pub type Metal = gfx_backend_metal::Backend;
    #[cfg(feature = "vulkan")]
    pub type Vulkan = gfx_backend_vulkan::Backend;

    #[cfg(feature = "dx12")]
    pub type Whatever = Dx12;

    #[cfg(feature = "metal")]
    pub type Whatever = Metal;

    #[cfg(feature = "vulkan")]
    pub type Whatever = Vulkan;

    #[cfg(all(
        feature = "opengl",
        not(any(feature = "metal", feature = "dx12", feature = "vulkan"))
    ))]
    pub type Whatever = OpenGL;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelRegion {
    pub upper_left: [u32; 2],
    pub lower_right: [u32; 2],
}

impl PixelRegion {
    pub const fn size(&self) -> [u32; 2] {
        let [x0, y0] = self.upper_left;
        let [x1, y1] = self.lower_right;
        [x1 - x0 + 1, y1 - y0 + 1]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub pixels: ([u32; 2], [u32; 2]),
    pub uv: ([f32; 2], [f32; 2]),
}

impl Region {
    pub const fn size(&self) -> [u32; 2] {
        self.pixels.1
    }

    pub const fn uv_size(&self) -> [f32; 2] {
        self.uv.1
    }

    pub fn sub(&self, size: [u32; 2], index: [usize; 2]) -> Self {
        let (px_pos, px_size) = self.pixels;
        let (uv_pos, uv_size) = self.uv;
        let tile_x = px_size[0] / size[0];
        let tile_y = px_size[1] / size[1];
        let sub_px_size = [px_size[0] / tile_x, px_size[1] / tile_y];
        let sub_uv_size = [uv_size[0] / tile_x as f32, uv_size[1] / tile_y as f32];
        let sub_px_pos = [
            px_pos[0] + sub_px_size[0] * index[0] as u32,
            px_pos[1] + sub_px_size[1] * index[1] as u32,
        ];
        let sub_uv_pos = [
            uv_pos[0] + sub_uv_size[0] * index[0] as f32,
            uv_pos[1] + sub_uv_size[1] * index[1] as f32,
        ];
        Region {
            pixels: (sub_px_pos, sub_px_size),
            uv: (sub_uv_pos, sub_uv_size),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphRegion {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv: ([f32; 2], [f32; 2]),
}

impl GlyphRegion {
    pub const fn size(&self) -> [f32; 2] {
        self.size
    }

    pub const fn uv_size(&self) -> [f32; 2] {
        self.uv.1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Depth(pub f32);

pub const D: Depth = Depth(1.);

impl std::ops::Add<Depth> for Depth {
    type Output = Depth;

    fn add(self, other: Depth) -> Self::Output {
        Depth(self.0 + other.0)
    }
}

impl std::ops::Mul<Depth> for f32 {
    type Output = Depth;

    fn mul(self, other: Depth) -> Self::Output {
        Depth(self * other.0)
    }
}

impl std::ops::Mul<Depth> for usize {
    type Output = Depth;

    fn mul(self, other: Depth) -> Self::Output {
        Depth(self as f32 * other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CanvasMode {
    Direct,
    Intermediate,
}

impl Default for CanvasMode {
    fn default() -> Self {
        CanvasMode::Direct
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResizeMode {
    Free,
    SetLogical([u32; 2]),
    SetPhysical([u32; 2]),
    Aspect([u32; 2]),
}

impl Default for ResizeMode {
    fn default() -> Self {
        ResizeMode::Free
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScaleMode {
    Set(f64),
    Max,
    MaxInt,
}

impl Default for ScaleMode {
    fn default() -> Self {
        ScaleMode::Set(1.)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct CanvasProperties {
    pub physical_canvas_size: [u32; 2],
    pub logical_canvas_size: [u32; 2],
    pub viewport_scissor_rect: ([i16; 2], [i16; 2]),
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CanvasConfig {
    pub canvas_mode: CanvasMode,
    pub resize_mode: ResizeMode,
    pub scale_mode: ScaleMode,
}

impl CanvasConfig {
    pub fn fixed(resolution: [u32; 2]) -> Self {
        CanvasConfig {
            canvas_mode: CanvasMode::Direct,
            resize_mode: ResizeMode::SetLogical(resolution),
            scale_mode: ScaleMode::Set(1.0),
        }
    }

    pub fn set_scaled(resolution: [u32; 2]) -> Self {
        CanvasConfig {
            canvas_mode: CanvasMode::Direct,
            resize_mode: ResizeMode::SetLogical(resolution),
            scale_mode: ScaleMode::Max,
        }
    }

    pub fn pixel_scaled(resolution: [u32; 2]) -> Self {
        CanvasConfig {
            canvas_mode: CanvasMode::Intermediate,
            resize_mode: ResizeMode::SetPhysical(resolution),
            scale_mode: ScaleMode::MaxInt,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn canvas_properties(
        &self,
        physical_window_size: [u32; 2],
        scale_factor: f64,
    ) -> CanvasProperties {
        let s = scale_factor;
        let [pw, ph] = physical_window_size;
        let logical_window_size = [
            (pw as f64 / scale_factor) as u32,
            (ph as f64 / scale_factor) as u32,
        ];

        fn fit_in(inner_size: [u32; 2], outer_size: [u32; 2]) -> [u32; 2] {
            let [ow, oh] = outer_size;
            let [iw, ih] = inner_size;
            let scaled_width = std::cmp::min(ow, (oh * iw) / ih);
            let scaled_height = std::cmp::min(oh, (ow * ih) / iw);
            [scaled_width, scaled_height]
        }

        let [cw, ch] = match self.resize_mode {
            ResizeMode::Free => physical_window_size,
            ResizeMode::SetLogical([w, h]) => [(w as f64 * s) as u32, (h as f64 * s) as u32],
            ResizeMode::SetPhysical(res) => res,
            ResizeMode::Aspect(aspect_ratio) => fit_in(aspect_ratio, physical_window_size),
        };

        let logical_canvas_size = match self.resize_mode {
            ResizeMode::Free => logical_window_size,
            ResizeMode::SetLogical(res) => res,
            ResizeMode::SetPhysical(res) => res,
            ResizeMode::Aspect(aspect_ratio) => fit_in(aspect_ratio, logical_window_size),
        };

        let [vw, vh] = match self.scale_mode {
            ScaleMode::Set(scale) => [(cw as f64 * scale) as u32, (ch as f64 * scale) as u32],
            ScaleMode::Max => fit_in([cw, ch], physical_window_size),
            ScaleMode::MaxInt => {
                let scale = std::cmp::min(pw as u32 / cw, ph as u32 / ch);
                match scale {
                    x if x > 0 => [cw * scale, ch * scale],
                    _ => fit_in([cw, ch], physical_window_size),
                }
            }
        };

        let viewport_inset = [
            (pw.saturating_sub(vw)) as i16 / 2,
            (ph.saturating_sub(vh)) as i16 / 2,
        ];

        CanvasProperties {
            physical_canvas_size: [cw, ch],
            logical_canvas_size,
            viewport_scissor_rect: (viewport_inset, [vw as i16, vh as i16]),
        }
    }
}
