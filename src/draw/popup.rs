// TODO:
// - Add canvas size/SF getters
// - Prove that mouse pointer location works
// - Handle resizes in example wrt projections
// - Add simple sprite API
// - Add very inefficient font rendering:
//  - FontAtlas into the 4th texture
//  - draw_glyphs API that converts each glyph to a trans draw call
// - Correctly set global uniforms
//
// then:
// - sensible font rendering that uses a smaller immediate-mode vertex buffer
//  - but is maybe part of the same render pass as trans stuff?
//
// then:
// - smarter asset management, allowing for efficient unloading/reloading
use std::collections::HashMap;

use image::RgbaImage;
use wgpu::util::DeviceExt;

use crate::{
    atlas::{
        font::FontAtlas, image::ImageAtlas, image_array::ImageArrayAtlas, mesh::MeshAtlas, Atlas,
    },
    draw::{CanvasConfig, Depth},
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
const DEBUG_SHADER: &'static str = include_str!("popup_debug_shader.wgsl");

const TEXTURES: usize = 4;
const TEXTURE_SIZE: u32 = 4096;
const MAX_VERTICES: usize = 65536;

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Vertex {
    position: [f32; 4],
    normal: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct VPush {
    transform: [[f32; 4]; 4],
    uv_offset_scale: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
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
    padding_0: f32,
    padding_1: f32,
    padding_2: f32,
}

fn push_constant_bytes<T>(t: &T) -> &[u8] {
    unsafe {
        let p = t as *const _ as *const u8;
        let size = std::mem::size_of::<T>();
        std::slice::from_raw_parts(p, size)
    }
}

pub struct Renderer<'a> {
    context: &'a mut DrawContext,
    clear_color: [f32; 4],
    generic_params: [f64; 4],
    // shader, image, mesh, ...
    opaque_calls: Vec<(String, String, String, Properties)>,
    trans_calls: Vec<(Depth, String, String, String, Properties)>,
    projection: Mat4<f32>,
    view: Mat4<f32>,
}

impl<'a> Renderer<'a> {
    fn render(&mut self) {
        let frame = self.context.surface.get_current_texture();
        match frame {
            Err(_) => self.context.surface_invalidated.set(),
            Ok(frame) => self.render_frame(frame),
        }
    }

    fn render_frame(&mut self, frame: wgpu::SurfaceTexture) {
        if self.context.mesh_atlas.modified() {
            let mut mesh = Mesh::<Vertex>::new();
            let updated_range = self.context.mesh_atlas.compile_into(&mut mesh).unwrap();

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

            self.context.queue.write_buffer(
                &self.context.vertex_buffer,
                vertex_offset as u64,
                vertex_data,
            );
            self.context.queue.write_buffer(
                &self.context.index_buffer,
                index_offset as u64,
                index_data,
            );
        }

        if self.context.image_atlas.modified() {
            let mut images: [_; TEXTURES - 1] =
                std::array::from_fn(|_| RgbaImage::new(TEXTURE_SIZE, TEXTURE_SIZE));
            self.context.image_atlas.compile_into(&mut images);

            for (i, image) in images.into_iter().enumerate() {
                self.context.queue.write_texture(
                    self.context.textures[i].0.as_image_copy(),
                    &image,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: std::num::NonZeroU32::new(4 * TEXTURE_SIZE),
                        rows_per_image: std::num::NonZeroU32::new(TEXTURE_SIZE),
                    },
                    wgpu::Extent3d {
                        width: TEXTURE_SIZE,
                        height: TEXTURE_SIZE,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        self.opaque_calls
            .sort_by(|call_a, call_b| (&call_a.0, &call_a.1).cmp(&(&call_b.0, &call_b.1)));
        self.trans_calls.sort_by(|call_a, call_b| {
            (call_a.0, &call_a.1, &call_a.2).cmp(&(call_b.0, &call_b.1, &call_b.2))
        });

        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // TODO: Upload globals

        // TODO: Do this only one time
        for index in 0..TEXTURES {
            let locals = LocalUniforms {
                texture_index: index as _,
                padding_0: 0.,
                padding_1: 0.,
                padding_2: 0.,
            };
            let uniform_bytes = unsafe {
                std::slice::from_raw_parts(
                    &locals as *const _ as *const u8,
                    std::mem::size_of::<LocalUniforms>(),
                )
            };

            self.context
                .queue
                .write_buffer(&self.context.local_buffers[index], 0, uniform_bytes);
        }

        let canvas_properties = self.context.canvas_config.canvas_properties(
            [
                self.context.surface_config.width,
                self.context.surface_config.height,
            ],
            self.context.scale_factor,
        );

        let mut commands = self
            .context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Opaque pass
        {
            let [r, g, b, a] = self.clear_color;
            let mut opaque_pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            });

            let ([x, y], [w, h]) = canvas_properties.viewport_scissor_rect;
            opaque_pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
            opaque_pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0., 1.);
            opaque_pass.set_bind_group(0, &self.context.global_bindings, &[]);
            opaque_pass.set_vertex_buffer(0, self.context.vertex_buffer.slice(..));
            opaque_pass.set_index_buffer(
                self.context.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );

            let mut active_pipeline = None;
            let mut active_binding = None;

            // TODO: Batch pipeline/image changes
            for call in &self.opaque_calls {
                let (shader, image, mesh, properties) = call;
                let (page, region) = self.context.image_atlas.fetch(image);
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

                if active_pipeline != Some(shader) {
                    let pipeline = &self.context.pipelines[shader][0];
                    opaque_pass.set_pipeline(pipeline);
                    active_pipeline = Some(shader);
                }

                if active_binding != Some(page) {
                    opaque_pass.set_bind_group(1, &self.context.local_bindings[page], &[]);
                    active_binding = Some(page);
                }

                opaque_pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX,
                    0,
                    push_constant_bytes(&vpush),
                );
                let push_offset = std::mem::size_of::<VPush>() as u32;
                opaque_pass.set_push_constants(
                    wgpu::ShaderStages::FRAGMENT,
                    push_offset,
                    push_constant_bytes(&fpush),
                );

                let mesh_index = self.context.mesh_atlas.fetch(mesh);
                opaque_pass.draw_indexed(mesh_index.index_range, 0, 0..1);
            }
        }

        // Transparent pass
        {
            let mut trans_pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            });

            let ([x, y], [w, h]) = canvas_properties.viewport_scissor_rect;
            trans_pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
            trans_pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0., 1.);
            trans_pass.set_bind_group(0, &self.context.global_bindings, &[]);
            trans_pass.set_vertex_buffer(0, self.context.vertex_buffer.slice(..));
            trans_pass.set_index_buffer(
                self.context.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );

            let mut active_pipeline = None;
            let mut active_binding = None;

            // TODO: Batch pipeline/image changes
            for call in &self.trans_calls {
                let (_depth, shader, image, mesh, properties) = call;
                let (page, region) = self.context.image_atlas.fetch(image);
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

                if active_pipeline != Some(shader) {
                    let pipeline = &self.context.pipelines[shader][1];
                    trans_pass.set_pipeline(pipeline);
                    active_pipeline = Some(shader);
                }

                if active_binding != Some(page) {
                    trans_pass.set_bind_group(1, &self.context.local_bindings[page], &[]);
                    active_binding = Some(page);
                }

                trans_pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX,
                    0,
                    push_constant_bytes(&vpush),
                );
                let push_offset = std::mem::size_of::<VPush>() as u32;
                trans_pass.set_push_constants(
                    wgpu::ShaderStages::FRAGMENT,
                    push_offset,
                    push_constant_bytes(&fpush),
                );

                let mesh_index = self.context.mesh_atlas.fetch(mesh);
                trans_pass.draw_indexed(mesh_index.index_range, 0, 0..1);
            }
        }

        self.context.queue.submit(Some(commands.finish()));
        frame.present();
    }

    pub fn set_projection(&mut self, matrix: [[f32; 4]; 4]) {
        self.projection = Mat4::new(matrix);
    }

    pub fn reset_projection(&mut self) {
        self.projection = Mat4::identity();
    }

    pub fn set_view(&mut self, matrix: [[f32; 4]; 4]) {
        self.view = Mat4::new(matrix);
    }

    pub fn modify_view(&mut self, matrix: [[f32; 4]; 4]) {
        self.view = Mat4::new(matrix) * self.view;
    }

    pub fn reset_view(&mut self) {
        self.view = Mat4::identity();
    }

    pub fn raw_opaque(
        &mut self,
        shader: &str,
        image: &str,
        mesh: &str,
        mut properties: Properties,
    ) {
        properties.transform = (self.projection * self.view * Mat4::from(properties.transform)).0;
        self.opaque_calls.push((
            shader.to_owned(),
            image.to_owned(),
            mesh.to_owned(),
            properties,
        ));
    }

    pub fn raw_trans(
        &mut self,
        depth: Depth,
        shader: &str,
        image: &str,
        mesh: &str,
        mut properties: Properties,
    ) {
        properties.transform = (self.projection * self.view * Mat4::from(properties.transform)).0;
        self.trans_calls.push((
            depth,
            shader.to_owned(),
            image.to_owned(),
            mesh.to_owned(),
            properties,
        ));
    }
}

impl<'a> Drop for Renderer<'a> {
    fn drop(&mut self) {
        self.render();
    }
}

pub struct DrawContext {
    canvas_config: CanvasConfig,
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

    textures: [(wgpu::Texture, wgpu::TextureView); TEXTURES],
    samplers: [wgpu::Sampler; TEXTURES],
    global_buffer: wgpu::Buffer,
    global_bindings: wgpu::BindGroup,
    local_buffers: [wgpu::Buffer; TEXTURES],
    local_bindings: [wgpu::BindGroup; TEXTURES],
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    mesh_atlas: MeshAtlas<String, Vertex>,
    image_atlas: ImageArrayAtlas<'static, str>,
    pipelines: HashMap<String, [wgpu::RenderPipeline; 2]>,
}

impl DrawContext {
    pub async fn new(
        window: &Window,
        canvas_config: CanvasConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(&window) };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
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

        let swapchain_format = surface.get_supported_formats(&adapter)[0];

        let size = window.inner_size();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
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
                    count: std::num::NonZeroU32::new(TEXTURES as u32),
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: std::num::NonZeroU32::new(TEXTURES as u32),
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
        let local_buffers: [_; TEXTURES] = std::array::from_fn(|i| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: local_size as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        let textures: [_; TEXTURES] = std::array::from_fn(|i| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width: TEXTURE_SIZE,
                    height: TEXTURE_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: None,
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            (texture, view)
        });
        let samplers: [wgpu::Sampler; TEXTURES] = std::array::from_fn(|i| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        });

        let global_bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: global_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureViewArray(&[
                        &textures[0].1,
                        &textures[1].1,
                        &textures[2].1,
                        &textures[3].1,
                    ]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::SamplerArray(&[
                        &samplers[0],
                        &samplers[1],
                        &samplers[2],
                        &samplers[3],
                    ]),
                },
            ],
            layout: &globals_layout,
            label: None,
        });

        let local_bindings: [_; TEXTURES] = std::array::from_fn(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: local_buffers[i].as_entire_binding(),
                }],
                layout: &locals_layout,
                label: None,
            })
        });

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
            image_atlas: ImageArrayAtlas::new([TEXTURE_SIZE; 2], Some(3)),
            pipelines: Default::default(),
        };

        let quad_mesh = Mesh {
            vertices: vec![
                Vertex {
                    position: [-0.5, -0.5, 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [0., 0., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
                Vertex {
                    position: [0.5, -0.5, 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [1., 0., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
                Vertex {
                    position: [-0.5, 0.5, 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [0., 1., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
                Vertex {
                    position: [0.5, 0.5, 0., 1.],
                    normal: [0., 0., 1., 0.],
                    uv: [1., 1., 0., 0.],
                    color: [1., 1., 1., 1.],
                },
            ],
            indices: vec![0, 1, 2, 1, 3, 2],
        };

        result.load_shader("builtin", BUILTIN_SHADER);
        result.load_shader("__debug", DEBUG_SHADER);
        result.load_mesh("quad", quad_mesh);
        result.load_image("pattern", {
            let bytes = include_bytes!("../../assets/images/pattern.png");
            image::load_from_memory(bytes).unwrap().to_rgba8()
        });
        result.load_image("pattern2", {
            let bytes = include_bytes!("../../assets/images/pattern2.png");
            image::load_from_memory(bytes).unwrap().to_rgba8()
        });
        result.load_image("pattern3", {
            let bytes = include_bytes!("../../assets/images/pattern3.png");
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

    pub fn load_mesh<N>(&mut self, name: N, mesh: Mesh<Vertex>)
    where
        N: AsRef<str>,
    {
        self.mesh_atlas.insert((name.as_ref().to_owned(), mesh));
    }

    pub fn load_image<N>(&mut self, name: N, image: RgbaImage)
    where
        N: AsRef<str>,
    {
        self.image_atlas.insert(name.as_ref().to_owned(), image);
    }

    pub fn load_shader<N, S>(&mut self, name: N, source: S)
    where
        N: AsRef<str>,
        S: AsRef<str>,
    {
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
        shader_source.push_str(source.as_ref());

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

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
                primitive: wgpu::PrimitiveState::default(),
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
                primitive: wgpu::PrimitiveState::default(),
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

        self.pipelines
            .insert(name.as_ref().to_owned(), [opaque_pipeline, trans_pipeline]);
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
            });

            let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.depth_buffer = Some(depth_view);
        }
    }

    pub fn start_rendering(&mut self, clear_color: [f32; 4], generic_params: [f64; 4]) -> Renderer {
        self.prepare_for_frame();
        Renderer {
            context: self,
            clear_color,
            generic_params,
            opaque_calls: Vec::with_capacity(128),
            trans_calls: Vec::with_capacity(128),
            projection: Mat4::identity(),
            view: Mat4::identity(),
        }
    }
}
