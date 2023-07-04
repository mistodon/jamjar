use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;

use hvec::HVec;
use image::RgbaImage;
use wgpu::util::DeviceExt;

use crate::{
    atlas::{font::FontAtlas, image_array::ImageArrayAtlas, mesh::MeshAtlas, Atlas},
    color,
    draw::{CanvasConfig, Depth, D},
    font::Glyph,
    layout::{Anchor, Frame},
    math::*,
    mesh::Mesh,
    utils::Flag,
    windowing::{
        event::{Event, WindowEvent},
        window::Window,
    },
};

const SHADER_HEADER: &'static str = include_str!("popup_shader_header.wgsl");
const BUILTIN_SHADER: &'static str = include_str!("popup_builtin_shader.wgsl");
const YFLIP_SHADER: &'static str = include_str!("popup_yflip_shader.wgsl");
const SIMPLELIGHT_SHADER: &'static str = include_str!("popup_simplelight_shader.wgsl");
const DEBUG_SHADER: &'static str = include_str!("popup_debug_shader.wgsl");

const SAMPLERS: usize = 2;

#[cfg(not(target_arch = "wasm32"))]
const MAX_VERTICES: usize = 65536;

// For some reason, WebGL doesn't support more than 16279 here(???)
#[cfg(target_arch = "wasm32")]
const MAX_VERTICES: usize = 10000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuiltinOnly {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuiltinImage {
    White,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuiltinMesh {
    Quad,
    Sprite,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuiltinShader {
    Basic,
    YFlip,
    SimpleLight,
    Debug,
}

type ImageAssetKey<K> = AssetKey<K, BuiltinImage>;
type MeshAssetKey<K> = AssetKey<K, BuiltinMesh>;
type ShaderAssetKey<K> = AssetKey<K, BuiltinShader>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AssetKey<K, B> {
    Key(K),
    Builtin(B),
}

impl<K: Clone> From<&K> for AssetKey<K, BuiltinImage> {
    fn from(key: &K) -> Self {
        AssetKey::Key(key.clone())
    }
}

impl<K> From<BuiltinImage> for AssetKey<K, BuiltinImage> {
    fn from(key: BuiltinImage) -> Self {
        AssetKey::Builtin(key)
    }
}

impl<K: Clone> From<&K> for AssetKey<K, BuiltinShader> {
    fn from(key: &K) -> Self {
        AssetKey::Key(key.clone())
    }
}

impl<K> From<BuiltinShader> for AssetKey<K, BuiltinShader> {
    fn from(key: BuiltinShader) -> Self {
        AssetKey::Builtin(key)
    }
}

impl<K: Clone> From<&K> for AssetKey<K, BuiltinMesh> {
    fn from(key: &K) -> Self {
        AssetKey::Key(key.clone())
    }
}

impl<K> From<BuiltinMesh> for AssetKey<K, BuiltinMesh> {
    fn from(key: BuiltinMesh) -> Self {
        AssetKey::Builtin(key)
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 4],
    pub normal: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
struct VPush {
    transform: [[f32; 4]; 4],
    uv_offset_scale: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
struct FPush {
    tint: [f32; 4],
    emission: [f32; 4],
    color_a: [f32; 4],
    color_b: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Properties {
    pub transform: [[f32; 4]; 4],
    pub tint: [f32; 4],
    pub emission: [f32; 4],
    pub color_a: [f32; 4],
    pub color_b: [f32; 4],
    pub pixel_texture: bool,
}

impl Default for Properties {
    fn default() -> Self {
        Properties {
            transform: Mat4::identity().0,
            tint: color::WHITE,
            emission: color::TRANS,
            color_a: [0.; 4],
            color_b: [0.; 4],
            pixel_texture: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct GlobalUniforms {
    view_vec: [f32; 4],
    generic_params: [f32; 4],
    pixel_size: [f32; 2],
    canvas_size: [f32; 2],
    texel_size: [f32; 2],
    cursor_pos: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct LocalUniforms {
    texture_index: u32,
    sampler_index: u32,
    padding_0: f32,
    padding_1: f32,
}

struct GlyphCtx {
    count: usize,
    depth: Depth,
    tint: [f32; 4],
    vp_matrix: Mat4<f32>,
}

fn push_constant_bytes<T>(t: &T) -> &[u8] {
    unsafe {
        let p = t as *const _ as *const u8;
        let size = std::mem::size_of::<T>();
        std::slice::from_raw_parts(p, size)
    }
}

#[derive(PartialEq)]
struct DrawCall {
    depth: Depth,
    shader_index: usize,
    binding_index: usize,
    index_range: Range<u32>,
    vpush: VPush,
    fpush: FPush,
}

impl PartialOrd for DrawCall {
    fn partial_cmp(&self, other: &DrawCall) -> Option<std::cmp::Ordering> {
        Some((self.depth, self.shader_index, self.binding_index).cmp(&(
            other.depth,
            other.shader_index,
            other.binding_index,
        )))
    }
}
impl Ord for DrawCall {
    fn cmp(&self, other: &DrawCall) -> std::cmp::Ordering {
        (self.depth, self.shader_index, self.binding_index).cmp(&(
            other.depth,
            other.shader_index,
            other.binding_index,
        ))
    }
}
impl Eq for DrawCall {}

pub struct Renderer<'a, ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    context: &'a mut DrawContext<ImageKey, MeshKey, ShaderKey>,
    clear_color: [f32; 4],
    generic_params: [f32; 4],
    cursor_pos: [f32; 2],
    opaque_calls: Vec<DrawCall>,
    trans_calls: Vec<DrawCall>,
    glyph_buffer: HVec,
    projection: Mat4<f32>,
    view: Mat4<f32>,
    vp_matrix: Mat4<f32>,
}

impl<'a, ImageKey, MeshKey, ShaderKey> Renderer<'a, ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    fn render(&mut self) {
        if self.context.image_atlas.modified() {
            self.context
                .image_atlas
                .compile_into(&mut self.context.image_atlas_images);

            for (i, image) in self.context.image_atlas_images.iter().enumerate() {
                self.context.queue.write_texture(
                    self.context.textures[i].0.as_image_copy(),
                    &image,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * self.context.texture_size),
                        rows_per_image: Some(self.context.texture_size),
                    },
                    wgpu::Extent3d {
                        width: self.context.texture_size,
                        height: self.context.texture_size,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        let frame = self.context.surface.get_current_texture();
        match frame {
            Err(_) => self.context.surface_invalidated.set(),
            Ok(frame) => self.render_frame(frame),
        }
    }

    fn render_frame(&mut self, frame: wgpu::SurfaceTexture) {
        self.prepare_glyphs();

        self.opaque_calls.sort();
        self.trans_calls.sort();

        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let canvas_properties = self.context.canvas_config.canvas_properties(
            [
                self.context.surface_config.width,
                self.context.surface_config.height,
            ],
            self.context.scale_factor,
        );

        let canvas_size = Vec2::from(canvas_properties.logical_canvas_size).as_f32();
        let globals = GlobalUniforms {
            view_vec: (self.view * vec4(0., 0., -1., 0.)).0,
            generic_params: Vec4::from(self.generic_params).as_f32().0,
            pixel_size: (vec2(2., 2.) / canvas_size).0,
            canvas_size: canvas_size.0,
            texel_size: [1. / self.context.texture_size as f32; 2],
            cursor_pos: self.cursor_pos,
        };

        let uniform_bytes = unsafe {
            std::slice::from_raw_parts(
                &globals as *const _ as *const u8,
                std::mem::size_of::<GlobalUniforms>(),
            )
        };

        self.context
            .queue
            .write_buffer(&self.context.global_buffer, 0, uniform_bytes);

        let mut commands = self
            .context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Passes
        for pass_num in 0..2 {
            let opaque = pass_num == 0;
            let [r, g, b, a] = self.clear_color;

            let mut pass = match opaque {
                true => commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: r.into(),
                                g: g.into(),
                                b: b.into(),
                                a: a.into(),
                            }),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: self.context.depth_buffer.as_ref().unwrap(),
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: true,
                        }),
                        stencil_ops: None,
                    }),
                }),
                false => commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: self.context.depth_buffer.as_ref().unwrap(),
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: false,
                        }),
                        stencil_ops: None,
                    }),
                }),
            };

            let ([x, y], [w, h]) = canvas_properties.viewport_scissor_rect;
            pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0., 1.);
            pass.set_bind_group(0, &self.context.global_bindings, &[]);
            pass.set_vertex_buffer(0, self.context.vertex_buffer.slice(..));
            pass.set_index_buffer(
                self.context.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );

            let mut active_pipeline = None;
            let mut active_binding = None;

            let calls = match opaque {
                true => &self.opaque_calls,
                false => &self.trans_calls,
            };

            for call in calls {
                if active_pipeline != Some(call.shader_index) {
                    let index = match opaque {
                        true => 0,
                        false => 1,
                    };
                    let pipeline = &self.context.pipelines[call.shader_index][index];
                    pass.set_pipeline(pipeline);
                    active_pipeline = Some(call.shader_index);
                }

                if active_binding != Some(call.binding_index) {
                    pass.set_bind_group(1, &self.context.local_bindings[call.binding_index], &[]);
                    active_binding = Some(call.binding_index);
                }

                pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX,
                    0,
                    push_constant_bytes(&call.vpush),
                );
                let push_offset = std::mem::size_of::<VPush>() as u32;
                pass.set_push_constants(
                    wgpu::ShaderStages::FRAGMENT,
                    push_offset,
                    push_constant_bytes(&call.fpush),
                );

                pass.draw_indexed(call.index_range.clone(), 0, 0..1);
            }
        }

        self.context.queue.submit(Some(commands.finish()));
        frame.present();
    }

    fn prepare_glyphs(&mut self) {
        let mut glyph_iter = self.glyph_buffer.iter();
        while let Some(ctx) = glyph_iter.next::<GlyphCtx>() {
            for _ in 0..ctx.count {
                let glyph = glyph_iter.next::<Glyph>().unwrap();
                self.context.font_atlas.insert(glyph.clone());
            }
        }

        if self.context.font_atlas.modified() {
            let upload = self
                .context
                .font_atlas
                .compile_into(&mut self.context.font_atlas_image);
            // TODO: Only upload change
            if let Some(_upload) = upload {
                self.context.queue.write_texture(
                    self.context.textures[self.context.texture_pages - 1]
                        .0
                        .as_image_copy(),
                    &self.context.font_atlas_image,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * self.context.texture_size),
                        rows_per_image: Some(self.context.texture_size),
                    },
                    wgpu::Extent3d {
                        width: self.context.texture_size,
                        height: self.context.texture_size,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        let mut glyph_buffer =
            std::mem::replace(&mut self.glyph_buffer, HVec::with_capacity(128, 128)).into_iter();

        while let Some(ctx) = glyph_buffer.next::<GlyphCtx>() {
            for _ in 0..ctx.count {
                let glyph = glyph_buffer.next::<Glyph>().unwrap();
                self.glyph_internal(ctx.depth, &glyph, ctx.tint, ctx.vp_matrix);
            }
        }
    }

    fn update_matrices(&mut self) {
        self.vp_matrix = self.projection * self.view;
    }

    pub fn reset_projection(&mut self) {
        self.projection = Mat4::identity();
        self.update_matrices();
    }

    pub fn reset_view(&mut self) {
        self.view = Mat4::identity();
        self.update_matrices();
    }

    pub fn set_projection(&mut self, matrix: [[f32; 4]; 4]) {
        self.projection = Mat4::new(matrix);
        self.update_matrices();
    }

    pub fn set_view(&mut self, matrix: [[f32; 4]; 4]) {
        self.view = Mat4::new(matrix);
        self.update_matrices();
    }

    pub fn modify_view(&mut self, matrix: [[f32; 4]; 4]) {
        self.view = Mat4::new(matrix) * self.view;
        self.update_matrices();
    }

    pub fn ortho_2d(&mut self) {
        let canvas_properties = self.context.canvas_config.canvas_properties(
            [
                self.context.surface_config.width,
                self.context.surface_config.height,
            ],
            self.context.scale_factor,
        );

        let [cx, cy] = canvas_properties.logical_canvas_size;
        let aspect = cx as f32 / cy as f32;
        let half_width = cx as f32 / 2.;
        let half_height = cy as f32 / 2.;

        self.projection = matrix::ortho_projection(aspect, half_height, -1., 1.);
        self.view = Mat4::translation([-half_width, -half_height, 0.]);
        self.update_matrices();
    }

    pub fn perspective_3d(&mut self, fov: f32) {
        let canvas_properties = self.context.canvas_config.canvas_properties(
            [
                self.context.surface_config.width,
                self.context.surface_config.height,
            ],
            self.context.scale_factor,
        );

        let [cx, cy] = canvas_properties.logical_canvas_size;
        let aspect = cx as f32 / cy as f32;

        self.projection = matrix::perspective_projection(aspect, fov, 0.1, 1000.);
        self.view = Mat4::identity();
        self.update_matrices();
    }

    pub fn stored_sprite<K: Into<ImageAssetKey<ImageKey>>>(
        &mut self,
        image: K,
        pos: [f32; 2],
        depth: Depth,
        pixelly: bool,
    ) -> Frame {
        let image = image.into();
        let (_page, region) = self.context.image_atlas.fetch(&image).unwrap();
        let [x, y] = pos;
        let [w, h] = region.size();
        let [w, h] = [w as f32, h as f32];
        self.raw_draw(
            BuiltinShader::YFlip,
            image,
            BuiltinMesh::Sprite,
            Properties {
                transform: (Mat4::translation([x, y, 0.]) * Mat4::scale([w, h, 1., 1.])).0,
                tint: color::WHITE,
                emission: color::TRANS,
                color_a: color::TRANS,
                color_b: color::TRANS,
                pixel_texture: pixelly,
            },
            Some(depth),
        );
        Anchor::from([x, y]).frame([w, h])
    }

    fn glyph_internal(
        &mut self,
        depth: Depth,
        glyph: &Glyph,
        tint: [f32; 4],
        vp_matrix: Mat4<f32>,
    ) {
        let region = self.context.font_atlas.fetch(glyph);
        if let Some(region) = region {
            let sf = self.context.scale_factor as f32;
            let pos = Vec2::new(region.pos) / sf;
            let size = Vec2::new(region.size) / sf;
            let [x, y] = pos.0;
            let [w, h] = size.0;
            let ([u, v], [us, vs]) = region.uv;

            let mesh_index = self
                .context
                .mesh_atlas
                .fetch(&BuiltinMesh::Sprite.into())
                .unwrap();

            self.trans_calls.push(DrawCall {
                depth,
                shader_index: self.context.shader_mapping[&BuiltinShader::YFlip.into()],
                binding_index: self.context.texture_pages - 1,
                index_range: mesh_index.index_range,
                vpush: VPush {
                    transform: (vp_matrix
                        * Mat4::translation([x, y, 0.])
                        * Mat4::scale([w, h, 1., 1.]))
                    .0, // TODO: bottleneck
                    uv_offset_scale: [u, v, us, vs],
                },
                fpush: FPush {
                    tint,
                    emission: color::TRANS,
                    color_a: color::TRANS,
                    color_b: color::TRANS,
                },
            });
        }
    }

    // TODO: Encode the VP matrix somehow :/
    pub fn glyphs<'g, I>(&mut self, glyphs: I, offset: [f32; 2], tint: [f32; 4], depth: Depth)
    where
        I: IntoIterator<Item = &'g Glyph>,
    {
        let sf = self.context.scale_factor as f32;
        let [dx, dy] = (Vec2::from(offset) * sf as f32).0;

        let glyphs = glyphs
            .into_iter()
            .map(|glyph| {
                let mut glyph = glyph.clone();
                let mut point = glyph.glyph.position();
                point.x += dx;
                point.y += dy;
                glyph.glyph.set_position(point);
                glyph
            })
            .collect::<Vec<_>>();
        let count = glyphs.len();

        self.glyph_buffer.push::<GlyphCtx>(GlyphCtx {
            count,
            depth,
            tint,
            vp_matrix: self.vp_matrix,
        });
        for glyph in glyphs {
            self.glyph_buffer.push(glyph);
        }
    }

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
        let [dx, dy] = (Vec2::from(offset) * sf as f32).0;

        let mut budget = budget;
        let mut drawn = 0;

        let mut done = true;
        let mut to_render = vec![];
        for glyph in glyphs {
            if budget <= 0. {
                done = false;
                break;
            }

            let ch = glyph.ch;
            let mut glyph = glyph.clone();
            let mut point = glyph.glyph.position();
            point.x += dx;
            point.y += dy;
            glyph.glyph.set_position(point);
            to_render.push(glyph);

            drawn += 1;
            let cost = cost_fn(ch);
            budget -= cost;
        }

        self.glyph_buffer.push(GlyphCtx {
            count: to_render.len(),
            depth,
            tint,
            vp_matrix: self.vp_matrix,
        });

        for glyph in to_render {
            self.glyph_buffer.push(glyph);
        }

        if done {
            (budget, None)
        } else {
            (0., Some(drawn))
        }
    }

    fn raw_internal(
        &mut self,
        shader: ShaderAssetKey<ShaderKey>,
        page: usize,
        sampler_index: usize,
        mesh: MeshAssetKey<MeshKey>,
        mut vpush: VPush,
        fpush: FPush,
        transparent_depth: Option<Depth>,
    ) {
        let shader_index = self.context.shader_mapping[&shader];
        let binding_index = page + sampler_index * self.context.texture_pages;

        let mesh_index = self.context.mesh_atlas.fetch(&mesh).unwrap();

        vpush.transform = (self.projection * self.view * Mat4::from(vpush.transform)).0; // TODO: bottleneck

        if let Some(depth) = transparent_depth {
            self.trans_calls.push(DrawCall {
                depth,
                shader_index,
                binding_index,
                index_range: mesh_index.index_range,
                vpush,
                fpush,
            });
        } else {
            self.opaque_calls.push(DrawCall {
                depth: 0 * D,
                shader_index,
                binding_index,
                index_range: mesh_index.index_range,
                vpush,
                fpush,
            });
        }
    }

    pub fn raw_draw<I, M, S>(
        &mut self,
        shader: S,
        image: I,
        mesh: M,
        properties: Properties,
        transparent_depth: Option<Depth>,
    ) where
        I: Into<ImageAssetKey<ImageKey>>,
        M: Into<MeshAssetKey<MeshKey>>,
        S: Into<ShaderAssetKey<ShaderKey>>,
    {
        let image = image.into();
        let sampler_index = if properties.pixel_texture { 1 } else { 0 };
        let (page, region) = self.context.image_atlas.fetch(&image).unwrap();
        let vpush = VPush {
            transform: properties.transform,
            uv_offset_scale: [
                region.uv.0[0],
                region.uv.0[1],
                region.uv.1[0],
                region.uv.1[1],
            ],
        };
        let fpush = FPush {
            tint: properties.tint,
            emission: properties.emission,
            color_a: properties.color_a,
            color_b: properties.color_b,
        };
        self.raw_internal(
            shader.into(),
            page,
            sampler_index,
            mesh.into(),
            vpush,
            fpush,
            transparent_depth,
        );
    }
}

impl<'a, ImageKey, MeshKey, ShaderKey> Renderer<'a, ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash + glace::Asset<Value = image::RgbaImage>,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    pub fn sprite<I>(&mut self, image: I, pos: [f32; 2], depth: Depth, pixelly: bool) -> Frame
    where
        I: Into<ImageAssetKey<ImageKey>>,
    {
        let image = image.into();
        if let AssetKey::Key(key) = &image {
            if self.context.image_atlas.fetch(&image).is_none() {
                self.context.load_image(key.clone(), key.value());
            }
        }
        self.stored_sprite(image, pos, depth, pixelly)
    }
}

impl<'a, ImageKey, MeshKey, ShaderKey> Renderer<'a, ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash + glace::Asset<Value = image::RgbaImage>,
    MeshKey: Clone + Eq + Hash + glace::Asset<Value = Cow<'static, [u8]>>,
    ShaderKey: Clone + Eq + Hash,
{
    pub fn draw<I, M, S>(
        &mut self,
        shader: S,
        image: I,
        mesh: M,
        properties: Properties,
        transparent_depth: Option<Depth>,
    ) where
        I: Into<ImageAssetKey<ImageKey>>,
        M: Into<MeshAssetKey<MeshKey>>,
        S: Into<ShaderAssetKey<ShaderKey>>,
    {
        let image = image.into();
        if let AssetKey::Key(key) = &image {
            if self.context.image_atlas.fetch(&image).is_none() {
                self.context.load_image(key.clone(), key.value());
            }
        }
        let mesh = mesh.into();
        if let AssetKey::Key(key) = &mesh {
            if self.context.mesh_atlas.fetch(&mesh).is_none() {
                self.context
                    .load_mesh(key.clone(), crate::mesh::load_glb(&key.value()).unwrap());
            }
        }
        self.raw_draw(shader, image, mesh, properties, transparent_depth)
    }
}

impl<'a, ImageKey, MeshKey, ShaderKey> Drop for Renderer<'a, ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    fn drop(&mut self) {
        self.render();
    }
}

#[allow(dead_code)]
pub struct DrawContext<ImageKey = BuiltinOnly, MeshKey = BuiltinOnly, ShaderKey = BuiltinOnly>
where
    ImageKey: Clone + Eq + Hash,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    canvas_config: CanvasConfig,
    texture_size: u32,
    texture_pages: usize,

    instance: wgpu::Instance,
    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    swapchain_format: wgpu::TextureFormat,
    surface_config: wgpu::SurfaceConfiguration,
    scale_factor: f64,
    surface_invalidated: Flag,
    depth_buffer: Option<wgpu::TextureView>,

    globals_layout: wgpu::BindGroupLayout,
    locals_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,

    textures: Vec<(wgpu::Texture, wgpu::TextureView)>,
    samplers: [wgpu::Sampler; 2],
    global_buffer: wgpu::Buffer,
    global_bindings: wgpu::BindGroup,
    local_buffers: Vec<wgpu::Buffer>,
    local_bindings: Vec<wgpu::BindGroup>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    mesh_atlas: MeshAtlas<MeshAssetKey<MeshKey>, Vertex>,
    image_atlas: ImageArrayAtlas<'static, ImageAssetKey<ImageKey>>,
    image_atlas_images: Vec<RgbaImage>,
    font_atlas: FontAtlas,
    font_atlas_image: RgbaImage,
    shader_mapping: HashMap<ShaderAssetKey<ShaderKey>, usize>,
    pipelines: Vec<[wgpu::RenderPipeline; 2]>,
}

impl<ImageKey, MeshKey, ShaderKey> DrawContext<ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    pub async fn new(
        window: &Window,
        canvas_config: CanvasConfig,
        texture_size: u32,
        texture_pages: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let texture_pages = texture_pages + 1; // Font texture
        let instance = wgpu::Instance::default();
        let surface = unsafe { instance.create_surface(&window).unwrap() };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::PUSH_CONSTANTS
                        | wgpu::Features::TEXTURE_BINDING_ARRAY,
                    limits: wgpu::Limits {
                        max_push_constant_size: 144,
                        ..wgpu::Limits::downlevel_webgl2_defaults()
                    }
                    .using_resolution(adapter.limits()),
                },
                None,
            )
            .await?;

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let size = window.inner_size();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![swapchain_format],
        };
        let scale_factor = window.scale_factor();

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<GlobalUniforms>() as _,
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: std::num::NonZeroU32::new(texture_pages as u32),
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: std::num::NonZeroU32::new(SAMPLERS as u32),
                },
            ],
        });

        let locals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<LocalUniforms>() as _
                    ),
                },
                count: None,
            }],
        });

        let vpush = std::mem::size_of::<VPush>() as u32;
        let fpush = std::mem::size_of::<FPush>() as u32;
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&globals_layout, &locals_layout],
            push_constant_ranges: &[
                wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX,
                    range: 0..vpush,
                },
                wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::FRAGMENT,
                    range: vpush..(vpush + fpush),
                },
            ],
        });

        let global_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<GlobalUniforms>() as _,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let local_size = std::mem::size_of::<LocalUniforms>();
        let num_buffers = texture_pages * SAMPLERS;
        let local_buffers = (0..num_buffers)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: local_size as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect::<Vec<_>>();

        let textures = (0..texture_pages)
            .map(|_| {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    size: wgpu::Extent3d {
                        width: texture_size,
                        height: texture_size,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    label: None,
                    view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
                });

                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

                (texture, view)
            })
            .collect::<Vec<_>>();

        let samplers: [_; SAMPLERS] = [
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }),
        ];

        let texture_views = textures.iter().map(|(_, view)| view).collect::<Vec<_>>();
        let global_bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: global_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureViewArray(&texture_views),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::SamplerArray(&[&samplers[0], &samplers[1]]),
                },
            ],
            layout: &globals_layout,
            label: None,
        });

        let num_bindings = texture_pages * SAMPLERS;
        let local_bindings = (0..num_bindings)
            .map(|i| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: local_buffers[i].as_entire_binding(),
                    }],
                    layout: &locals_layout,
                    label: None,
                })
            })
            .collect::<Vec<_>>();

        for sampler_index in 0..SAMPLERS {
            for index in 0..texture_pages {
                let locals = LocalUniforms {
                    texture_index: index as _,
                    sampler_index: sampler_index as _,
                    padding_0: 0.,
                    padding_1: 0.,
                };

                let uniform_bytes = unsafe {
                    std::slice::from_raw_parts(
                        &locals as *const _ as *const u8,
                        std::mem::size_of::<LocalUniforms>(),
                    )
                };
                queue.write_buffer(
                    &local_buffers[index + sampler_index * texture_pages],
                    0,
                    uniform_bytes,
                );
            }
        }

        let vertex_bytes = [0; MAX_VERTICES * std::mem::size_of::<Vertex>()];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &vertex_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
        });

        let index_bytes = [0; MAX_VERTICES * std::mem::size_of::<u16>()];
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &index_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::INDEX,
        });

        let mut result = DrawContext {
            canvas_config,
            texture_size,
            texture_pages,

            instance,
            surface,
            adapter,
            device,
            queue,
            swapchain_format,
            surface_config,
            scale_factor,
            surface_invalidated: Flag::new(true),
            depth_buffer: None,

            globals_layout,
            locals_layout,
            pipeline_layout,

            textures,
            samplers,
            global_buffer,
            global_bindings,
            local_buffers,
            local_bindings,
            vertex_buffer,
            index_buffer,

            mesh_atlas: MeshAtlas::new(),
            image_atlas: ImageArrayAtlas::new([texture_size; 2], Some(3)),
            image_atlas_images: vec![RgbaImage::new(texture_size, texture_size); texture_pages - 1],
            font_atlas: FontAtlas::with_size([texture_size; 2]),
            font_atlas_image: RgbaImage::new(texture_size, texture_size),
            shader_mapping: Default::default(),
            pipelines: Default::default(),
        };

        let quad_mesh = Mesh {
            vertices: vec![
                Vertex {
                    position: [-0.5, -0.5, 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [0., 1., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
                Vertex {
                    position: [0.5, -0.5, 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [1., 1., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
                Vertex {
                    position: [-0.5, 0.5, 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [0., 0., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
                Vertex {
                    position: [0.5, 0.5, 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [1., 0., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
            ],
            indices: vec![0, 1, 2, 1, 3, 2],
        };

        let sprite_mesh = Mesh {
            vertices: vec![
                Vertex {
                    position: [0., 0., 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [0., 0., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
                Vertex {
                    position: [1., 0., 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [1., 0., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
                Vertex {
                    position: [0., 1., 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [0., 1., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
                Vertex {
                    position: [1., 1., 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [1., 1., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
            ],
            indices: vec![0, 1, 2, 1, 3, 2],
        };

        result.load_shader_internal(
            AssetKey::Builtin(BuiltinShader::Basic),
            BUILTIN_SHADER,
            false,
        );
        result.load_shader_internal(
            AssetKey::Builtin(BuiltinShader::SimpleLight),
            SIMPLELIGHT_SHADER,
            false,
        );
        result.load_shader_internal(AssetKey::Builtin(BuiltinShader::YFlip), YFLIP_SHADER, true);
        result.load_shader_internal(AssetKey::Builtin(BuiltinShader::Debug), DEBUG_SHADER, false);

        result.load_mesh_internal(BuiltinMesh::Quad, quad_mesh);
        result.load_mesh_internal(BuiltinMesh::Sprite, sprite_mesh);

        result.load_image_internal(BuiltinImage::White, {
            let bytes = include_bytes!("../../assets/images/white.png");
            image::load_from_memory(bytes).unwrap().to_rgba8()
        });

        Ok(result)
    }

    pub fn handle_winit_event(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(dims),
                ..
            } => self.resized((*dims).into()),
            Event::WindowEvent {
                event:
                    WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        new_inner_size,
                    },
                ..
            } => self.scale_factor_changed(*scale_factor, (**new_inner_size).into()),
            _ => (),
        }
    }

    pub fn resized(&mut self, new_inner_size: (u32, u32)) {
        self.surface_config.width = new_inner_size.0;
        self.surface_config.height = new_inner_size.1;
        self.surface_invalidated.set();
    }

    pub fn scale_factor_changed(&mut self, scale_factor: f64, new_inner_size: (u32, u32)) {
        self.surface_config.width = new_inner_size.0;
        self.surface_config.height = new_inner_size.1;
        self.scale_factor = scale_factor;
        self.surface_invalidated.set();
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    pub fn canvas_properties(&self) -> crate::draw::CanvasProperties {
        self.canvas_config.canvas_properties(
            [self.surface_config.width, self.surface_config.height],
            self.scale_factor,
        )
    }

    pub fn window_to_canvas_pos(&self, window_pos: [f64; 2]) -> Option<[f32; 2]> {
        let canvas_properties = self.canvas_config.canvas_properties(
            [self.surface_config.width, self.surface_config.height],
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

    pub fn load_mesh(&mut self, key: MeshKey, mesh: Mesh<Vertex>) {
        self.mesh_atlas.insert((AssetKey::Key(key), mesh));
    }

    fn load_mesh_internal(&mut self, key: BuiltinMesh, mesh: Mesh<Vertex>) {
        self.mesh_atlas.insert((AssetKey::Builtin(key), mesh));
    }

    pub fn load_image(&mut self, key: ImageKey, image: RgbaImage) {
        self.image_atlas.insert(AssetKey::Key(key), image);
    }

    fn load_image_internal(&mut self, key: BuiltinImage, image: RgbaImage) {
        self.image_atlas.insert(AssetKey::Builtin(key), image);
    }

    pub fn load_shader<S>(&mut self, key: ShaderKey, source: S, y_flipped: bool)
    where
        S: AsRef<str>,
    {
        self.load_shader_internal(AssetKey::Key(key), source.as_ref(), y_flipped)
    }

    fn load_shader_internal(
        &mut self,
        name: ShaderAssetKey<ShaderKey>,
        source: &str,
        y_flipped: bool,
    ) {
        let vertex_attributes = wgpu::vertex_attr_array![
            0 => Float32x4,
            1 => Float32x4,
            2 => Float32x4,
            3 => Float32x4,
        ];
        let vertex_size = std::mem::size_of::<Vertex>();
        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: vertex_size as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vertex_attributes,
        };

        let mut shader_source = SHADER_HEADER.to_owned();
        shader_source.push_str(source);

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let primitive = wgpu::PrimitiveState {
            front_face: if y_flipped {
                wgpu::FrontFace::Cw
            } else {
                wgpu::FrontFace::Ccw
            },
            cull_mode: Some(wgpu::Face::Back),
            ..wgpu::PrimitiveState::default()
        };

        let opaque_pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vertex_main",
                    buffers: &[vertex_buffer_layout.clone()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fragment_main",
                    targets: &[Some(self.swapchain_format.into())],
                }),
                primitive,
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            });

        let trans_pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vertex_main",
                    buffers: &[vertex_buffer_layout.clone()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fragment_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.swapchain_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::default(),
                    })],
                }),
                primitive,
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            });

        let shader_index = self.pipelines.len();
        self.shader_mapping.insert(name, shader_index);
        self.pipelines.push([opaque_pipeline, trans_pipeline]);
    }

    fn prepare_for_frame(&mut self) {
        if self.surface_invalidated.check() {
            self.surface.configure(&self.device, &self.surface_config);

            let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: self.surface_config.width,
                    height: self.surface_config.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[wgpu::TextureFormat::Depth32Float],
            });

            let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.depth_buffer = Some(depth_view);
        }

        if self.mesh_atlas.modified() {
            let mut mesh = Mesh::<Vertex>::new();
            let updated_range = self.mesh_atlas.compile_into(&mut mesh).unwrap();

            let vertex_offset =
                updated_range.vertex_range.start as usize * std::mem::size_of::<Vertex>();
            let index_offset =
                updated_range.index_range.start as usize * std::mem::size_of::<u16>();

            // TODO: Less hacky alignment fix
            if (mesh.indices.len() % 2) != 0 {
                mesh.indices.push(0);
            }

            let vertex_data = unsafe {
                std::slice::from_raw_parts(
                    mesh.vertices.as_ptr() as *const Vertex as *const u8,
                    mesh.vertices.len() * std::mem::size_of::<Vertex>(),
                )
            };
            let index_data = unsafe {
                std::slice::from_raw_parts(
                    mesh.indices.as_ptr() as *const u16 as *const u8,
                    mesh.indices.len() * std::mem::size_of::<u16>(),
                )
            };

            self.queue
                .write_buffer(&self.vertex_buffer, vertex_offset as u64, vertex_data);
            self.queue
                .write_buffer(&self.index_buffer, index_offset as u64, index_data);
        }
    }

    pub fn start_rendering(
        &mut self,
        clear_color: [f32; 4],
        cursor_pos: [f32; 2],
        generic_params: [f32; 4],
    ) -> Renderer<ImageKey, MeshKey, ShaderKey> {
        self.prepare_for_frame();

        Renderer {
            context: self,
            clear_color,
            generic_params,
            cursor_pos,
            opaque_calls: Vec::with_capacity(128),
            trans_calls: Vec::with_capacity(128),
            glyph_buffer: HVec::with_capacity(128, 128),
            projection: Mat4::identity(),
            view: Mat4::identity(),
            vp_matrix: Mat4::identity(),
        }
    }
}
