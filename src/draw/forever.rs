#![allow(warnings)]

use std::mem::ManuallyDrop;

use image::RgbaImage;

use crate::{
    draw::{CanvasConfig, CanvasMode, Depth, GlyphRegion, PixelRegion, Region},
    gfx::{self, easy, prelude::*, SupportedBackend},
    math::*,
    utils::over,
    windowing::{
        dpi::{LogicalSize, PhysicalSize},
        window::Window,
    },
};

#[cfg(feature = "font")]
use crate::{atlas::Atlas, font::Glyph};

#[cfg(all(target_arch = "wasm32", not(feature = "opengl")))]
compile_error!("Web builds (wasm32) require the `opengl` feature to be enabled.");

#[cfg(not(all(target_arch = "wasm32", feature = "bypass_spirv_cross")))]
const SHADER_SOURCES: (&'static [u8], &'static [u8]) = (
    include_bytes!("../../assets/shaders/compiled/forever.vert.spv"),
    include_bytes!("../../assets/shaders/compiled/forever.frag.spv"),
);

#[cfg(all(target_arch = "wasm32", feature = "bypass_spirv_cross"))]
const SHADER_SOURCES: (&'static [u8], &'static [u8]) = (
    include_bytes!("../../assets/shaders/compiled/forever.es.vert"),
    include_bytes!("../../assets/shaders/compiled/forever.es.frag"),
);

pub const MAX_SPRITES: usize = 100000;
const VERTEX_BUFFER_LEN: usize = MAX_SPRITES * 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub depth: Depth,
    pub corners: [[f32; 2]; 4],
    pub tint: [f32; 4],
    pub atlas_uv: ([f32; 2], [f32; 2]),
}

impl Sprite {
    pub fn simple(region: Region, pos: [f32; 2]) -> Self {
        Self::tinted(region, pos, [1., 1., 1., 1.])
    }

    pub fn tinted(region: Region, pos: [f32; 2], tint: [f32; 4]) -> Self {
        Self::scaled(region, pos, tint, [1., 1.])
    }

    pub fn scaled(region: Region, pos: [f32; 2], tint: [f32; 4], scale: [f32; 2]) -> Self {
        let (_, size) = region.pixels;
        let size = Vec2::new(scale) * Vec2::new(size).as_f32();

        Self::sized(region, pos, tint, size.0)
    }

    pub fn sized(region: Region, pos: [f32; 2], tint: [f32; 4], size: [f32; 2]) -> Self {
        let [x, y] = pos;
        let [w, h] = size;

        Sprite {
            depth: Depth(0.),
            corners: [[x, y], [x, y + h], [x + w, y + h], [x + w, y]],
            tint,
            atlas_uv: region.uv,
        }
    }

    pub fn rotated(
        region: Region,
        center: [f32; 2],
        tint: [f32; 4],
        scale: [f32; 2],
        radians: f32,
    ) -> Self {
        let (_, size) = region.pixels;
        let center = Vec2::new(center);
        let extent = (Vec2::new(scale) * Vec2::new(size).as_f32()) / 2.;
        let ortho = extent * vec2(1., -1.);
        let (s, c) = radians.sin_cos();
        let rot = Mat2::new([[c, -s], [s, c]]);

        Sprite {
            depth: Depth(0.),
            corners: [
                (rot * (center - extent)).0,
                (rot * (center - ortho)).0,
                (rot * (center + extent)).0,
                (rot * (center + ortho)).0,
            ],
            tint,
            atlas_uv: region.uv,
        }
    }

    pub fn glyph(
        region: GlyphRegion,
        tint: [f32; 4],
        display_scale_factor: f64,
        depth: Depth,
    ) -> Self {
        let sf = display_scale_factor as f32;
        let pos = Vec2::new(region.pos) / sf;
        let size = Vec2::new(region.size) / sf;
        let [x, y] = pos.0;
        let [w, h] = size.0;
        Sprite {
            depth,
            corners: [[x, y], [x, y + h], [x + w, y + h], [x + w, y]],
            tint,
            atlas_uv: region.uv,
        }
    }

    pub fn line(region: Region, a: [f32; 2], b: [f32; 2], thickness: f32, tint: [f32; 4]) -> Self {
        let a = Vec2::new(a);
        let d = Vec2::new(b) - a;
        let ortho = vec2(d.0[1], -d.0[0]).norm_zero() * (thickness / 2.);
        let tl = a + ortho;
        let bl = a - ortho;
        Sprite {
            depth: Depth(0.),
            corners: [tl.0, bl.0, (bl + d).0, (tl + d).0],
            tint,
            atlas_uv: region.uv,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[repr(C)]
struct Vertex {
    pub tint: [f32; 4],
    pub uv: [f32; 2],
    pub offset: [f32; 3],
}

fn wiperr<T>(_: T) -> () {}

fn is_srgb(surface_format: Format) -> bool {
    surface_format.base_format().1 == hal::format::ChannelType::Srgb
}

fn texture_format(surface_format: Format) -> Format {
    if is_srgb(surface_format) {
        Format::Rgba8Srgb
    } else {
        Format::Rgba8Unorm
    }
}

struct Resources<B: SupportedBackend> {
    _instance: Option<B::Instance>,
    surface: B::Surface,
    command_pool: B::CommandPool,
    vertex_buffer: (B::Memory, B::Buffer),
    atlas_image: (B::Memory, B::Image, B::ImageView),
    sampler: B::Sampler,
    render_pass_to_canvas: B::RenderPass,
    desc_set_layout: B::DescriptorSetLayout,
    desc_set_pool: B::DescriptorPool,
    desc_set: B::DescriptorSet,
    blit_desc_set: B::DescriptorSet,
    pipeline_layout_to_canvas: B::PipelineLayout,
    pipeline_to_canvas: B::GraphicsPipeline,
    submission_complete_fence: B::Fence,
    rendering_complete_semaphore: B::Semaphore,
    intermediate_canvas: (B::Memory, B::Image, B::ImageView),
    intermediate_canvas_size: [u32; 2],
    render_pass_to_surface: B::RenderPass,
    pipeline_layout_to_surface: B::PipelineLayout,
    pipeline_to_surface: B::GraphicsPipeline,
}

pub struct DrawContext<B: SupportedBackend> {
    resources: ManuallyDrop<Resources<B>>,
    adapter: Adapter<B>,
    device: B::Device,
    queue_group: QueueGroup<B>,
    command_buffer: B::CommandBuffer,
    surface_color_format: hal::format::Format,
    surface_extent: hal::window::Extent2D,
    scale_factor: f64,
    framebuffer_attachment: Option<FramebufferAttachment>,
    swapchain_invalidated: Option<()>,
    texture_atlas: RgbaImage,
    vertex_cache: Vec<Vertex>,
    canvas_config: CanvasConfig,
}

impl<B: SupportedBackend> DrawContext<B> {
    pub fn new(
        window: &Window,
        canvas_config: CanvasConfig,
        texture_atlas: RgbaImage,
        linear_filter: bool,
    ) -> Result<Self, ()> {
        let (
            instance,
            surface,
            surface_color_format,
            adapter,
            device,
            mut queue_group,
            mut command_pool,
        ) = easy::init::<B>(window, "jamjar_forever", 1)
            .map_err(|msg| eprintln!("easy::init error: {}", msg))?;

        let mut command_buffer = unsafe { command_pool.allocate_one(hal::command::Level::Primary) };

        let dpi = window.scale_factor();
        let physical_size: PhysicalSize<u32> = window.inner_size();
        let logical_size: LogicalSize<u32> = physical_size.to_logical(dpi);
        let mut surface_extent = hal::window::Extent2D {
            width: physical_size.width,
            height: physical_size.height,
        };

        let vertex_buffer = unsafe {
            gfx::make_buffer::<B>(
                &device,
                &adapter.physical_device,
                VERTEX_BUFFER_LEN * std::mem::size_of::<Vertex>(),
                hal::buffer::Usage::VERTEX,
                hal::memory::Properties::CPU_VISIBLE,
            )
        };

        let atlas_image_size = texture_atlas.dimensions();
        let atlas_image = unsafe {
            use hal::format::{Aspects, Format};
            use hal::image::Usage;

            gfx::make_image::<B>(
                &device,
                &adapter.physical_device,
                atlas_image_size,
                texture_format(surface_color_format),
                Usage::SAMPLED | Usage::TRANSFER_DST,
                Aspects::COLOR,
            )
        };

        let (intermediate_canvas, intermediate_canvas_size) = unsafe {
            use gfx_hal::format::{Aspects, Format};
            use gfx_hal::image::Usage;

            let canvas_properties =
                canvas_config.canvas_properties([surface_extent.width, surface_extent.height], dpi);

            let intermediate_canvas_size = canvas_properties.physical_canvas_size;

            (
                gfx::make_image::<B>(
                    &device,
                    &adapter.physical_device,
                    (intermediate_canvas_size[0], intermediate_canvas_size[1]),
                    Format::Rgba8Srgb,
                    Usage::COLOR_ATTACHMENT | Usage::SAMPLED,
                    Aspects::COLOR,
                ),
                intermediate_canvas_size,
            )
        };

        let sampler = unsafe {
            use hal::image::{Filter, SamplerDesc, Usage, WrapMode};

            let filter = match linear_filter {
                true => Filter::Linear,
                false => Filter::Nearest,
            };

            device
                .create_sampler(&SamplerDesc::new(filter, WrapMode::Clamp))
                .expect("TODO")
        };

        unsafe {
            gfx::upload_image::<B>(
                &device,
                &adapter.physical_device,
                &mut command_pool,
                &mut queue_group.queues[0],
                &atlas_image.1,
                atlas_image_size,
                &texture_atlas,
            );
        }

        let render_pass_to_canvas = easy::render_pass::<B>(&device, Format::Rgba8Srgb, None, true);
        let render_pass_to_surface =
            easy::render_pass::<B>(&device, surface_color_format, None, false);

        let (desc_set_layout, mut desc_set_pool, mut desc_sets) = easy::desc_sets::<B>(
            &device,
            vec![
                (vec![], vec![&atlas_image.2], vec![&sampler]),
                (vec![], vec![&intermediate_canvas.2], vec![&sampler]),
            ],
        );

        let mut desc_set = desc_sets.remove(0);
        let mut blit_desc_set = desc_sets.remove(0);

        let (pipeline_to_canvas, pipeline_layout_to_canvas) = easy::pipeline::<B>(
            &device,
            Some(&desc_set_layout),
            0,
            SHADER_SOURCES.0,
            SHADER_SOURCES.1,
            &render_pass_to_canvas,
            None,
            &[4, 2, 3],
        );

        let (pipeline_to_surface, pipeline_layout_to_surface) = easy::pipeline::<B>(
            &device,
            Some(&desc_set_layout),
            0,
            SHADER_SOURCES.0,
            SHADER_SOURCES.1,
            &render_pass_to_surface,
            None,
            &[4, 2, 3],
        );

        let submission_complete_fence = device.create_fence(true).expect("Out of memory");
        let rendering_complete_semaphore = device.create_semaphore().expect("Out of memory");

        Ok(DrawContext {
            resources: ManuallyDrop::new(Resources {
                _instance: Some(instance),
                surface,
                command_pool,
                vertex_buffer,
                atlas_image,
                sampler,
                render_pass_to_canvas,
                desc_set_layout,
                desc_set_pool,
                desc_set,
                blit_desc_set,
                pipeline_layout_to_canvas,
                pipeline_to_canvas,
                submission_complete_fence,
                rendering_complete_semaphore,
                intermediate_canvas,
                intermediate_canvas_size,
                render_pass_to_surface,
                pipeline_layout_to_surface,
                pipeline_to_surface,
            }),
            adapter,
            device,
            queue_group,
            command_buffer,
            surface_color_format,
            surface_extent,
            scale_factor: dpi,
            framebuffer_attachment: None,
            swapchain_invalidated: Some(()),
            texture_atlas,
            vertex_cache: Vec::with_capacity(VERTEX_BUFFER_LEN),
            canvas_config,
        })
    }

    pub fn resolution_changed(&mut self, resolution: (u32, u32)) {
        self.surface_extent = hal::window::Extent2D {
            width: resolution.0,
            height: resolution.1,
        };
        self.swapchain_invalidated = Some(());
    }

    pub fn scale_factor_changed(&mut self, scale_factor: f64, resolution: (u32, u32)) {
        self.scale_factor = scale_factor;
        self.resolution_changed(resolution);
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    pub fn set_canvas_config(&mut self, canvas_config: CanvasConfig) {
        self.canvas_config = canvas_config;
    }

    pub fn window_to_canvas_pos(&self, window_pos: [f64; 2]) -> Option<[f32; 2]> {
        let canvas_properties = self.canvas_config.canvas_properties(
            [self.surface_extent.width, self.surface_extent.height],
            self.scale_factor,
        );

        let [x, y] = window_pos;
        let [cw, ch] = canvas_properties.logical_canvas_size;
        let ([ox, oy], [w, h]) = canvas_properties.viewport_scissor_rect;
        let (ox, oy, w, h) = (ox as f64, oy as f64, w as f64, h as f64);

        let pos = [
            (((x - ox) / w) * cw as f64) as f32,
            (((y - oy) / h) * ch as f64) as f32,
        ];
        (pos[0] >= 0. && pos[0] <= cw as f32 && pos[1] >= 0. && pos[1] <= ch as f32).then(|| pos)
    }

    pub fn start_rendering(&mut self, clear_color: Color) -> Renderer<B> {
        let Resources {
            surface,
            submission_complete_fence,
            command_pool,
            render_pass_to_canvas,
            render_pass_to_surface,
            intermediate_canvas,
            intermediate_canvas_size,
            sampler,
            blit_desc_set,
            ..
        } = &mut *self.resources;

        unsafe {
            use hal::pool::CommandPool;

            // We refuse to wait more than a second, to avoid hanging.
            let render_timeout_ns = 1_000_000_000;

            self.device
                .wait_for_fence(&submission_complete_fence, render_timeout_ns)
                .expect("Out of memory or device lost");

            self.device
                .reset_fence(submission_complete_fence)
                .expect("Out of memory");

            command_pool.reset(false);
        }

        if self.swapchain_invalidated.take().is_some() {
            self.framebuffer_attachment = Some(easy::reconfigure_swapchain::<B>(
                surface,
                &self.adapter,
                &self.device,
                self.surface_color_format,
                &mut self.surface_extent,
            ));
        }

        let canvas_properties = self.canvas_config.canvas_properties(
            [self.surface_extent.width, self.surface_extent.height],
            self.scale_factor,
        );

        if canvas_properties.physical_canvas_size != *intermediate_canvas_size {
            *intermediate_canvas_size = canvas_properties.physical_canvas_size;

            unsafe {
                use gfx_hal::format::{Aspects, Format};
                use gfx_hal::image::Usage;

                let replacement_image = gfx::make_image::<B>(
                    &self.device,
                    &self.adapter.physical_device,
                    (intermediate_canvas_size[0], intermediate_canvas_size[1]),
                    Format::Rgba8Srgb,
                    Usage::COLOR_ATTACHMENT | Usage::SAMPLED,
                    Aspects::COLOR,
                );

                {
                    let (mem, img, view) =
                        std::mem::replace(intermediate_canvas, replacement_image);
                    self.device.destroy_image_view(view);
                    self.device.destroy_image(img);
                    self.device.free_memory(mem);
                }

                easy::write_desc_sets::<B>(
                    &self.device,
                    vec![blit_desc_set],
                    vec![(vec![], vec![&intermediate_canvas.2], vec![&sampler])],
                );
            }
        }

        let framebuffer_to_canvas = unsafe {
            use gfx_hal::image::Extent;

            self.device
                .create_framebuffer(
                    render_pass_to_canvas,
                    self.framebuffer_attachment.iter().cloned(),
                    Extent {
                        width: self.surface_extent.width,
                        height: self.surface_extent.height,
                        depth: 1,
                    },
                )
                .unwrap()
        };

        let framebuffer_to_surface = easy::acquire_framebuffer::<B>(
            &self.device,
            surface,
            &self.surface_extent,
            &render_pass_to_surface,
            self.framebuffer_attachment.clone().unwrap(),
        );

        let framebuffer_to_surface = match framebuffer_to_surface {
            Ok(x) => Some(x),
            Err(msg) => {
                eprintln!("easy::acquire_framebuffer: {:?}", msg);
                self.swapchain_invalidated = Some(());
                None
            }
        };

        let mut renderer = Renderer {
            context: self,
            clear_color,
            camera: [0., 0.],
            camera_depth: Depth(0.),
            framebuffer_to_canvas,
            framebuffer_to_surface,
            sprites: vec![
                Sprite {
                    depth: Depth(0.),
                    corners: Default::default(),
                    tint: [0., 0., 0., 0.],
                    atlas_uv: ([0., 0.], [0., 0.]),
                }, // Note: Dummy sprite for fullscreen quad
            ],

            #[cfg(feature = "font")]
            glyphs: vec![],
        };
        renderer
    }
}

impl<B: SupportedBackend> Drop for DrawContext<B> {
    fn drop(&mut self) {
        unsafe {
            let Resources {
                _instance,
                mut surface,
                command_pool,
                vertex_buffer,
                atlas_image,
                sampler,
                render_pass_to_canvas,
                desc_set_layout,
                desc_set_pool,
                pipeline_layout_to_canvas,
                pipeline_to_canvas,
                submission_complete_fence,
                rendering_complete_semaphore,
                intermediate_canvas,
                intermediate_canvas_size,
                render_pass_to_surface,
                pipeline_layout_to_surface,
                pipeline_to_surface,
                desc_set,
                blit_desc_set,
            } = ManuallyDrop::take(&mut self.resources);

            self.device.destroy_semaphore(rendering_complete_semaphore);
            self.device.destroy_fence(submission_complete_fence);
            self.device.destroy_graphics_pipeline(pipeline_to_canvas);
            self.device
                .destroy_pipeline_layout(pipeline_layout_to_canvas);
            self.device.destroy_descriptor_pool(desc_set_pool);
            self.device.destroy_descriptor_set_layout(desc_set_layout);
            self.device.destroy_render_pass(render_pass_to_canvas);
            self.device.destroy_sampler(sampler);
            {
                let (mem, img, view) = atlas_image;
                self.device.destroy_image_view(view);
                self.device.destroy_image(img);
                self.device.free_memory(mem);
            }
            {
                let (mem, buf) = vertex_buffer;
                self.device.destroy_buffer(buf);
                self.device.free_memory(mem);
            }
            self.device.destroy_command_pool(command_pool);
            surface.unconfigure_swapchain(&self.device);
            if let Some(instance) = _instance {
                instance.destroy_surface(surface);
            }
        }
    }
}

pub struct Renderer<'a, B: SupportedBackend> {
    context: &'a mut DrawContext<B>,
    clear_color: Color,
    camera: [f32; 2],
    camera_depth: Depth,
    framebuffer_to_canvas: B::Framebuffer,
    framebuffer_to_surface: Option<(
        B::Framebuffer,
        <B::Surface as PresentationSurface<B>>::SwapchainImage,
        Viewport,
    )>,
    sprites: Vec<Sprite>,

    #[cfg(feature = "font")]
    glyphs: Vec<(Glyph, [f32; 4], Depth)>,
}

impl<'a, B: SupportedBackend> Renderer<'a, B> {
    pub fn set_camera(&mut self, pos: [f32; 2]) {
        self.camera = pos;
    }

    pub fn clear_camera(&mut self) {
        self.camera = [0., 0.];
    }

    pub fn set_depth(&mut self, depth: Depth) {
        self.camera_depth = depth;
    }

    pub fn clear_depth(&mut self) {
        self.camera_depth = Depth(0.);
    }

    pub fn add_depth(&mut self, amount: Depth) {
        self.camera_depth = self.camera_depth + amount;
    }

    pub fn sprite(&mut self, mut sprite: Sprite) {
        let [dx, dy] = self.camera;
        for p in &mut sprite.corners {
            p[0] -= dx;
            p[1] -= dy;
        }
        sprite.depth = sprite.depth + self.camera_depth;
        self.sprites.push(sprite);
    }

    // TODO: Can we maybe just _borrow_ Glyphs instead of
    // cloning them all the damn time?
    #[cfg(feature = "font")]
    pub fn glyphs<'g, I>(&mut self, glyphs: I, offset: [f32; 2], tint: [f32; 4])
    where
        I: IntoIterator<Item = &'g Glyph>,
    {
        let sf = self.context.scale_factor as f32;

        let [dx, dy] = (Vec2::new(offset) - Vec2::new(self.camera)).0;

        for glyph in glyphs {
            let mut glyph = glyph.clone();
            let mut point = glyph.glyph.position();
            point.x += dx * sf;
            point.y += dy * sf;
            glyph.glyph.set_position(point);
            self.glyphs.push((glyph, tint, Depth(0.)));
        }
    }

    #[cfg(feature = "font")]
    pub fn glyphs_d<'g, I>(&mut self, glyphs: I, offset: [f32; 2], tint: [f32; 4], depth: Depth)
    where
        I: IntoIterator<Item = &'g Glyph>,
    {
        let sf = self.context.scale_factor as f32;

        let [dx, dy] = (Vec2::new(offset) - Vec2::new(self.camera)).0;

        for glyph in glyphs {
            let mut glyph = glyph.clone();
            let mut point = glyph.glyph.position();
            point.x += dx * sf;
            point.y += dy * sf;
            glyph.glyph.set_position(point);
            self.glyphs.push((glyph, tint, depth + self.camera_depth));
        }
    }

    #[cfg(feature = "font")]
    pub fn glyphs_partial<'g, I, F: Fn(char) -> f64>(
        &mut self,
        glyphs: I,
        offset: [f32; 2],
        tint: [f32; 4],
        depth: Depth,
        budget: f64,
        cost_fn: F,
    ) -> (f64, Option<usize>)
    where
        I: IntoIterator<Item = &'g Glyph>,
    {
        let sf = self.context.scale_factor as f32;

        let [dx, dy] = (Vec2::new(offset) - Vec2::new(self.camera)).0;

        let mut budget = budget;
        let mut drawn = 0;

        for glyph in glyphs {
            if budget <= 0. {
                return (0., Some(drawn));
            }

            let ch = glyph.ch;
            let mut glyph = glyph.clone();
            let mut point = glyph.glyph.position();
            point.x += dx * sf;
            point.y += dy * sf;
            glyph.glyph.set_position(point);
            self.glyphs.push((glyph, tint, depth + self.camera_depth));

            drawn += 1;
            let cost = cost_fn(ch);
            budget -= cost;
        }

        (budget, None)
    }

    #[cfg(feature = "font")]
    pub fn finish_with_text<A>(mut self, font_atlas: &mut A, atlas_image: Option<&mut RgbaImage>)
    where
        A: Atlas<Glyph, Glyph, Option<GlyphRegion>, RgbaImage, PixelRegion>,
    {
        for (glyph, _, _) in &self.glyphs {
            font_atlas.insert(glyph.clone());
        }

        let (upload, atlas_image) = if font_atlas.modified() {
            match atlas_image {
                Some(dest) => (font_atlas.compile_into(dest), Some(dest)),
                None => (
                    font_atlas.compile_into(&mut self.context.texture_atlas),
                    None,
                ),
            }
        } else {
            (None, atlas_image)
        };

        if let Some(atlas_image) = atlas_image {
            self.context.texture_atlas = atlas_image.clone();
        }

        if let Some(region) = upload {
            self.upload_atlas_partial(region);
        }

        for (glyph, tint, depth) in self.glyphs.drain(..) {
            let glyph_region = font_atlas.fetch(&glyph);
            if let Some(glyph_region) = glyph_region {
                let glyph_sprite =
                    Sprite::glyph(glyph_region, tint, self.context.scale_factor, depth);
                self.sprites.push(glyph_sprite);
            }
        }
    }

    fn upload_atlas_partial(&mut self, region: PixelRegion) {
        let dimensions = self.context.texture_atlas.dimensions();

        let Resources {
            command_pool,
            atlas_image,
            ..
        } = &mut *self.context.resources;

        let row_size = dimensions.0 as usize * 4;
        let row_range = region.upper_left[1]..=region.lower_right[1];
        let start_byte = *row_range.start() as usize * row_size;
        let end_byte = (row_range.end() + 1) as usize * row_size;
        let byte_range = start_byte..end_byte;

        unsafe {
            gfx::upload_image_part::<B>(
                &self.context.device,
                &self.context.adapter.physical_device,
                command_pool,
                &mut self.context.queue_group.queues[0],
                &atlas_image.1,
                dimensions.0,
                row_range,
                &self.context.texture_atlas.as_ref()[byte_range],
            );
        }
    }

    pub fn update_atlas(&mut self, new_atlas: RgbaImage) {
        let dimensions = new_atlas.dimensions();
        self.context.texture_atlas = new_atlas;

        let Resources {
            command_pool,
            atlas_image,
            ..
        } = &mut *self.context.resources;

        unsafe {
            gfx::upload_image::<B>(
                &self.context.device,
                &self.context.adapter.physical_device,
                command_pool,
                &mut self.context.queue_group.queues[0],
                &atlas_image.1,
                dimensions,
                &self.context.texture_atlas,
            );
        }
    }
}

impl<'a, B: SupportedBackend> Drop for Renderer<'a, B> {
    fn drop(&mut self) {
        #[cfg(feature = "font")]
        {
            assert!(self.glyphs.is_empty(), "Renderer dropped without handling text. (Use `finish_with_text()` before dropping.)");
        }

        let Resources {
            command_pool,
            vertex_buffer,
            surface,
            submission_complete_fence,
            rendering_complete_semaphore,
            pipeline_layout_to_canvas,
            pipeline_to_canvas,
            render_pass_to_canvas,
            intermediate_canvas,
            pipeline_layout_to_surface,
            pipeline_to_surface,
            render_pass_to_surface,
            desc_set,
            blit_desc_set,
            ..
        } = &mut *self.context.resources;

        let canvas_properties = self.context.canvas_config.canvas_properties(
            [
                self.context.surface_extent.width,
                self.context.surface_extent.height,
            ],
            self.context.scale_factor,
        );

        // TODO: Dynamically grow vertex buffer?
        assert!(self.sprites.len() <= MAX_SPRITES, "Too many sprites being rendered - currently jamjar cannot dynamically grow the sprite buffer.");

        let verts = &mut self.context.vertex_cache;
        verts.clear(); // TODO: Maybe actually cache?

        let [canvas_width, canvas_height] = canvas_properties.logical_canvas_size;

        let scale_x = (2.0 / canvas_width as f64) as f32;
        let scale_y = (2.0 / canvas_height as f64) as f32;

        let project = |[x, y]: [f32; 2]| {
            #[cfg(all(target_arch = "wasm32", feature = "bypass_spirv_cross"))]
            {
                [(x * scale_x) - 1., -1. * ((y * scale_y) - 1.), 0.]
            }
            #[cfg(not(all(target_arch = "wasm32", feature = "bypass_spirv_cross")))]
            {
                [(x * scale_x) - 1., (y * scale_y) - 1., 0.]
            }
        };

        self.sprites
            .sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap());

        for sprite in &self.sprites {
            let tint = if is_srgb(self.context.surface_color_format) {
                gfx::srgb_to_linear(sprite.tint)
            } else {
                sprite.tint
            };
            let corners = sprite.corners;
            let ([u0, v0], [uw, vh]) = sprite.atlas_uv;
            let p0 = Vertex {
                offset: project(corners[0]),
                tint: tint,
                uv: [u0, v0],
            };
            let p1 = Vertex {
                offset: project(corners[1]),
                tint: tint,
                uv: [u0, v0 + vh],
            };
            let p2 = Vertex {
                offset: project(corners[2]),
                tint: tint,
                uv: [u0 + uw, v0 + vh],
            };
            let p3 = Vertex {
                offset: project(corners[3]),
                tint: tint,
                uv: [u0 + uw, v0],
            };
            verts.push(p0);
            verts.push(p1);
            verts.push(p2);
            verts.push(p0);
            verts.push(p2);
            verts.push(p3);
        }

        let white = [1., 1., 1., 1.];
        let flip = {
            #[cfg(all(target_arch = "wasm32", feature = "bypass_spirv_cross"))]
            {
                -1.0
            }
            #[cfg(not(all(target_arch = "wasm32", feature = "bypass_spirv_cross")))]
            {
                1.0
            }
        };
        verts[0] = Vertex {
            offset: [-1., -1. * flip, 0.],
            uv: [0., 0.],
            tint: white,
        };
        verts[1] = Vertex {
            offset: [-1., 1. * flip, 0.],
            uv: [0., 1.],
            tint: white,
        };
        verts[2] = Vertex {
            offset: [1., 1. * flip, 0.],
            uv: [1., 1.],
            tint: white,
        };
        verts[3] = Vertex {
            offset: [-1., -1. * flip, 0.],
            uv: [0., 0.],
            tint: white,
        };
        verts[4] = Vertex {
            offset: [1., 1. * flip, 0.],
            uv: [1., 1.],
            tint: white,
        };
        verts[5] = Vertex {
            offset: [1., -1. * flip, 0.],
            uv: [1., 0.],
            tint: white,
        };

        // Upload to vertex buffer
        let vertex_bytes = verts.len() * std::mem::size_of::<Vertex>();
        unsafe {
            use gfx_hal::memory::Segment;

            let (memory, buffer) = vertex_buffer;
            let segment = Segment {
                offset: 0,
                size: Some(vertex_bytes as u64),
            };
            let mapped_memory = self
                .context
                .device
                .map_memory(memory, segment.clone())
                .expect("Failed to map memory");

            std::ptr::copy_nonoverlapping(verts.as_ptr() as *const u8, mapped_memory, vertex_bytes);

            self.context
                .device
                .flush_mapped_memory_ranges(over([(&*memory, segment)]))
                .expect("Out of memory");

            self.context.device.unmap_memory(memory);
        }

        let ([x, y], [w, h]) = canvas_properties.viewport_scissor_rect;

        let canvas_viewport = Viewport {
            rect: hal::pso::Rect {
                x: 0,
                y: 0,
                w: canvas_properties.physical_canvas_size[0] as i16,
                h: canvas_properties.physical_canvas_size[1] as i16,
            },
            depth: 0.0..1.0,
        };

        let surface_viewport = Viewport {
            rect: hal::pso::Rect { x, y, w, h },
            depth: 0.0..1.0,
        };

        let intermediate_mode = match self.context.canvas_config.canvas_mode {
            CanvasMode::Intermediate if cfg!(target_arch = "wasm32") => false,
            CanvasMode::Intermediate => true,
            CanvasMode::Direct => false,
        };

        if let Some((framebuffer, surface_image, _)) = self.framebuffer_to_surface.take() {
            use std::borrow::Borrow;

            unsafe {
                use hal::command::{
                    ClearColor, ClearValue, CommandBuffer, CommandBufferFlags, SubpassContents,
                };

                let (first_pass, second_pass) = match intermediate_mode {
                    true => (
                        (
                            canvas_viewport,
                            render_pass_to_canvas,
                            &self.framebuffer_to_canvas,
                            pipeline_to_canvas,
                            pipeline_layout_to_canvas,
                        ),
                        Some((
                            surface_viewport,
                            render_pass_to_surface,
                            &framebuffer,
                            pipeline_to_surface,
                            pipeline_layout_to_surface,
                        )),
                    ),
                    false => (
                        (
                            surface_viewport,
                            render_pass_to_surface,
                            &framebuffer,
                            pipeline_to_surface,
                            pipeline_layout_to_surface,
                        ),
                        None,
                    ),
                };

                // Draw sprites
                {
                    let (viewport, render_pass, mode_framebuffer, pipeline, pipeline_layout) =
                        first_pass;

                    self.context
                        .command_buffer
                        .begin_primary(CommandBufferFlags::ONE_TIME_SUBMIT);

                    self.context
                        .command_buffer
                        .set_viewports(0, over([viewport.clone()]));

                    self.context
                        .command_buffer
                        .set_scissors(0, over([viewport.rect]));

                    self.context.command_buffer.begin_render_pass(
                        render_pass,
                        mode_framebuffer,
                        viewport.rect,
                        over([RenderAttachmentInfo {
                            image_view: if intermediate_mode {
                                &intermediate_canvas.2
                            } else {
                                surface_image.borrow()
                            },
                            clear_value: ClearValue {
                                color: ClearColor {
                                    float32: self.clear_color,
                                },
                            },
                        }]),
                        SubpassContents::Inline,
                    );

                    self.context.command_buffer.bind_graphics_descriptor_sets(
                        pipeline_layout,
                        0,
                        over([&*desc_set]),
                        over([]),
                    );

                    self.context.command_buffer.bind_vertex_buffers(
                        0,
                        over([(
                            &vertex_buffer.1,
                            gfx_hal::buffer::SubRange {
                                offset: 0,
                                size: Some(vertex_bytes as u64),
                            },
                        )]),
                    );

                    self.context.command_buffer.bind_graphics_pipeline(pipeline);

                    let num_verts = verts.len() as u32;
                    self.context.command_buffer.draw(6..num_verts, 0..1);

                    self.context.command_buffer.end_render_pass();
                }

                if let Some((viewport, render_pass, mode_framebuffer, pipeline, pipeline_layout)) =
                    second_pass
                {
                    {
                        use gfx_hal::image::Access;
                        use gfx_hal::memory::{Barrier, Dependencies};
                        use gfx_hal::pso::PipelineStage;

                        self.context.command_buffer.pipeline_barrier(
                            PipelineStage::all()..PipelineStage::all(),
                            Dependencies::empty(),
                            over([Barrier::AllImages(
                                Access::SHADER_READ..Access::SHADER_WRITE,
                            )]),
                        );
                    }

                    self.context
                        .command_buffer
                        .set_viewports(0, over([viewport.clone()]));
                    self.context
                        .command_buffer
                        .set_scissors(0, over([viewport.rect]));

                    self.context.command_buffer.begin_render_pass(
                        render_pass,
                        mode_framebuffer,
                        viewport.rect,
                        over([RenderAttachmentInfo {
                            image_view: surface_image.borrow(),
                            clear_value: ClearValue {
                                color: ClearColor {
                                    float32: self.clear_color,
                                },
                            },
                        }]),
                        SubpassContents::Inline,
                    );

                    self.context.command_buffer.bind_graphics_pipeline(pipeline);

                    self.context.command_buffer.bind_graphics_descriptor_sets(
                        pipeline_layout,
                        0,
                        over([&*blit_desc_set]),
                        over([]),
                    );

                    self.context.command_buffer.draw(0..6, 0..1);

                    self.context.command_buffer.end_render_pass();
                }

                self.context.command_buffer.finish();

                use hal::queue::Queue;

                self.context.queue_group.queues[0].submit(
                    over([&self.context.command_buffer]),
                    over([]),
                    over([&*rendering_complete_semaphore]),
                    Some(submission_complete_fence),
                );

                let result = self.context.queue_group.queues[0].present(
                    surface,
                    surface_image,
                    Some(rendering_complete_semaphore),
                );

                if result.is_err() {
                    self.context.swapchain_invalidated = Some(());
                }

                self.context.device.destroy_framebuffer(framebuffer);
                // NOTE: Umm... I don't know why the next line is commented out...
                // self.context.device.destroy_framebuffer(framebuffer_to_canvas);
            }
        }
    }
}
