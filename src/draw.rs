#[cfg(feature = "draw_forever")]
pub mod forever;

#[cfg(feature = "draw_groove")]
pub mod groove;

#[cfg(feature = "draw_sloth")]
pub mod sloth;

#[cfg(feature = "draw_popup")]
pub mod popup;

#[cfg(feature = "draw_cherry")]
pub mod cherry;

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
pub struct Depth(f32);

impl Depth {
    pub fn new(value: f32) -> Self {
        assert!(value.is_normal());
        Depth(value)
    }
}

impl Eq for Depth {}

impl Ord for Depth {
    fn cmp(&self, other: &Depth) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

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
    SetLogical([f32; 2]),
    SetLogicalWidth(f32),
    SetLogicalHeight(f32),
    SetLogicalMin(f32),
    SetPhysical([u32; 2]),
    SetPhysicalWidth(u32),
    SetPhysicalHeight(u32),
    SetPhysicalMin(u32),
    Aspect([u32; 2]),
}

impl Default for ResizeMode {
    fn default() -> Self {
        ResizeMode::Free
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScaleMode {
    Set(f32),
    Max,
    MaxInt,
}

impl Default for ScaleMode {
    fn default() -> Self {
        ScaleMode::Set(1.)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CanvasProperties {
    pub physical_canvas_size: [u32; 2],
    pub logical_canvas_size: [f32; 2],
    pub viewport_scissor_rect: ([i16; 2], [i16; 2]),
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CanvasConfig {
    pub canvas_mode: CanvasMode,
    pub resize_mode: ResizeMode,
    pub scale_mode: ScaleMode,
}

impl CanvasConfig {
    pub fn fixed(resolution: [f32; 2]) -> Self {
        CanvasConfig {
            canvas_mode: CanvasMode::Direct,
            resize_mode: ResizeMode::SetLogical(resolution),
            scale_mode: ScaleMode::Set(1.0),
        }
    }

    pub fn set_scaled(resolution: [f32; 2]) -> Self {
        CanvasConfig {
            canvas_mode: CanvasMode::Direct,
            resize_mode: ResizeMode::SetLogical(resolution),
            scale_mode: ScaleMode::Max,
        }
    }

    pub fn set_width(width: f32) -> Self {
        CanvasConfig {
            canvas_mode: CanvasMode::Direct,
            resize_mode: ResizeMode::SetLogicalWidth(width),
            scale_mode: ScaleMode::Max,
        }
    }

    pub fn set_height(height: f32) -> Self {
        CanvasConfig {
            canvas_mode: CanvasMode::Direct,
            resize_mode: ResizeMode::SetLogicalHeight(height),
            scale_mode: ScaleMode::Max,
        }
    }

    pub fn set_min(size: f32) -> Self {
        CanvasConfig {
            canvas_mode: CanvasMode::Direct,
            resize_mode: ResizeMode::SetLogicalMin(size),
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
    pub fn canvas_properties(
        &self,
        physical_window_size: [u32; 2],
        scale_factor: f64,
    ) -> CanvasProperties {
        // TODO: Fix rounding errors when scaling int sizes (esp small ints)
        let s = scale_factor;
        let [pw, ph] = physical_window_size;
        let [pw, ph] = [pw as f64, ph as f64];
        let logical_window_size = [pw / scale_factor, ph / scale_factor];
        let window_aspect = pw / ph;
        let i_window_aspect = 1. / window_aspect;

        fn fit_in(inner_size: [f64; 2], outer_size: [f64; 2]) -> [f64; 2] {
            let [ow, oh] = outer_size;
            let [iw, ih] = inner_size;
            let scaled_width = ow.min((oh * iw) / ih);
            let scaled_height = oh.min((ow * ih) / iw);
            [scaled_width, scaled_height]
        }

        let physical_canvas_size: [f64; 2] = match self.resize_mode {
            ResizeMode::Free => [
                physical_window_size[0] as f64,
                physical_window_size[1] as f64,
            ],
            ResizeMode::SetLogical([w, h]) => [w as f64 * s, h as f64 * s],
            ResizeMode::SetLogicalWidth(w) => [w as f64 * s, w as f64 * s * i_window_aspect],
            ResizeMode::SetLogicalHeight(h) => [h as f64 * s * window_aspect, h as f64 * s],
            ResizeMode::SetLogicalMin(v) => {
                let v = v as f64 * s;
                match pw > ph {
                    true => [v * window_aspect, v],
                    false => [v, v * i_window_aspect],
                }
            }
            ResizeMode::SetPhysical(res) => [res[0] as f64, res[1] as f64],
            ResizeMode::SetPhysicalWidth(w) => [w as f64, w as f64 * i_window_aspect],
            ResizeMode::SetPhysicalHeight(h) => [h as f64 * window_aspect, h as f64],
            ResizeMode::SetPhysicalMin(v) => match pw > ph {
                true => [v as f64 * window_aspect, v as f64],
                false => [v as f64, v as f64 * i_window_aspect],
            },
            ResizeMode::Aspect(aspect_ratio) => {
                let [w, h] = fit_in([aspect_ratio[0] as f64, aspect_ratio[1] as f64], [pw, ph]);
                [w as f64, h as f64]
            }
        };

        let [cw, ch] = physical_canvas_size;

        let logical_canvas_size: [f64; 2] = match self.resize_mode {
            ResizeMode::Free => logical_window_size,
            ResizeMode::SetLogical(res) => [res[0] as f64, res[1] as f64],
            ResizeMode::SetLogicalWidth(w) => [w as f64, w as f64 * i_window_aspect],
            ResizeMode::SetLogicalHeight(h) => [h as f64 * window_aspect, h as f64],
            ResizeMode::SetLogicalMin(v) => match pw > ph {
                true => [v as f64 * window_aspect, v as f64],
                false => [v as f64, v as f64 * i_window_aspect],
            },
            ResizeMode::SetPhysical(res) => [res[0] as f64, res[1] as f64],
            ResizeMode::SetPhysicalWidth(w) => [w as f64, w as f64 * i_window_aspect],
            ResizeMode::SetPhysicalHeight(h) => [h as f64 * window_aspect, h as f64],
            ResizeMode::SetPhysicalMin(v) => match pw > ph {
                true => [v as f64 * window_aspect, v as f64],
                false => [v as f64, v as f64 * i_window_aspect],
            },
            ResizeMode::Aspect(aspect_ratio) => fit_in(
                [aspect_ratio[0] as f64, aspect_ratio[1] as f64],
                logical_window_size,
            ),
        };

        let physical_viewport_size: [u32; 2] = match self.scale_mode {
            ScaleMode::Set(scale) => [(cw * scale as f64) as u32, (ch * scale as f64) as u32],
            ScaleMode::Max => {
                let fit = fit_in([cw, ch], [pw, ph]);
                [fit[0] as u32, fit[1] as u32]
            }
            ScaleMode::MaxInt => {
                let scale = (pw / cw).min(ph / ch).floor();
                match scale {
                    x if x > 0. => [(cw * scale) as u32, (ch * scale) as u32],
                    _ => {
                        let fit = fit_in([cw, ch], [pw, ph]);
                        [fit[0] as u32, fit[1] as u32]
                    }
                }
            }
        };

        let [vw, vh] = physical_viewport_size;

        let viewport_inset = [
            (physical_window_size[0].saturating_sub(vw)) as i16 / 2,
            (physical_window_size[1].saturating_sub(vh)) as i16 / 2,
        ];

        CanvasProperties {
            physical_canvas_size: [cw as u32, ch as u32],
            logical_canvas_size: [logical_canvas_size[0] as f32, logical_canvas_size[1] as f32],
            viewport_scissor_rect: (viewport_inset, [vw as i16, vh as i16]),
        }
    }
}
