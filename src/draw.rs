pub mod backend {
    #[cfg(feature = "opengl")]
    pub type OpenGL = gfx_backend_gl::Backend;
    #[cfg(feature = "metal")]
    pub type Metal = gfx_backend_metal::Backend;

    #[cfg(feature = "metal")]
    pub type Whatever = Metal;

    #[cfg(all(feature = "opengl", not(any(feature = "metal"))))]
    pub type Whatever = OpenGL;
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
            canvas_mode: CanvasMode::Intermediate,
            resize_mode: ResizeMode::SetLogical(resolution),
            scale_mode: ScaleMode::Set(1.0),
        }
    }

    pub fn set_scaled(resolution: [u32; 2]) -> Self {
        CanvasConfig {
            canvas_mode: CanvasMode::Intermediate,
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

    pub(crate) fn canvas_properties(
        &self,
        physical_window_size: [u32; 2],
        scale_factor: f64,
    ) -> CanvasProperties {
        let s = scale_factor;
        let [pw, ph] = physical_window_size;

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

        let viewport_inset = [(pw.saturating_sub(vw)) as i16 / 2, (ph.saturating_sub(vh)) as i16 / 2];

        CanvasProperties {
            physical_canvas_size: [cw, ch],
            viewport_scissor_rect: (viewport_inset, [vw as i16, vh as i16]),
        }
    }
}
