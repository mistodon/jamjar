use std::{
    any::TypeId,
    borrow::Cow,
    collections::HashMap,
    hash::Hash,
    ops::Range,
    path::PathBuf,
    sync::Arc,
    time::SystemTime,
};

#[cfg(not(web_platform))]
use std::time as timecrate;

#[cfg(web_platform)]
use web_time as timecrate;

use timecrate::{Duration, Instant};

use hvec::HVec;
use image::RgbaImage;
use wgpu::util::DeviceExt;

use crate::{
    atlas::{font::FontAtlas, image_array::ImageArrayAtlas, mesh::SubmeshAtlas, Atlas},
    color,
    draw::{CanvasConfig, Depth, D},
    font::Glyph,
    layout::{Anchor, Frame, Pivot},
    math::*,
    mesh::{Mesh, Submeshes, Vertex},
    utils::Flag,
    windowing::{
        event::{Event, WindowEvent},
        window::Window,
    },
};

const SHADER_HEADER: &'static str = include_str!("cherry_shaders/header.wgsl");
const BUILTIN_SHADER: &'static str = include_str!("cherry_shaders/builtin.wgsl");
const YFLIP_SHADER: &'static str = include_str!("cherry_shaders/yflip.wgsl");
const SIMPLE_SHADER: &'static str = include_str!("cherry_shaders/simple.wgsl");
const LIT_SHADER: &'static str = include_str!("cherry_shaders/lit.wgsl");

const SAMPLERS: u8 = 2;

#[cfg(not(web_platform))]
const MAX_VERTICES: usize = 65536;

// For some reason, WebGL doesn't support more than 16279 here(???)
#[cfg(web_platform)]
const MAX_VERTICES: usize = 10000;

// TODO: Use this later
#[cfg(android_platform)]
const DEFER_CREATE_SURFACE: bool = true;

#[cfg(not(android_platform))]
const DEFER_CREATE_SURFACE: bool = false;

#[derive(Debug, Default, Clone)]
pub struct RenderStats {
    pub frame: usize,
    pub camera_passes: usize,
    pub pipeline_changes: usize,
    pub binding_changes: usize,
    pub uniform_changes: usize,

    pub opaque_objects: usize,
    pub transparent_objects: usize,
    pub uniform_types: usize,
    pub uniform_buffers: usize,

    pub opaque_bytes: usize,
    pub transparent_bytes: usize,
    pub glyph_bytes: usize,
    pub push_constant_bytes: usize,
    pub uniform_bytes: usize,

    pub total_cpu_bytes: usize,

    pub frame_time: Duration,
    pub prepare_time: Duration,
    pub render_time: Duration,
    pub submit_time: Duration,
    pub present_time: Duration,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ShaderConf {
    pub phase: i8,
    pub shader_flags: ShaderFlags,
    pub push_flags: PushFlags,
}

bitflags::bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ShaderFlags: u32 {
        const Y_FLIPPED = 1 << 0;
        const NO_COLOR_WRITE = 1 << 1;
        const NO_DEPTH_WRITE = 1 << 2;
        const BLEND_ADD = 1 << 3;
        const BACK_FACE_ONLY = 1 << 4;
        const STENCIL_ADD = 1 << 5;
        const STENCIL_SUB = 1 << 6;
        const STENCIL_SHOWS = 1 << 7;
        const STENCIL_HIDES = 1 << 8;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct PushFlags: u32 {
        const TRANSFORM = 1 << 0;
        const MODEL_MATRIX = 1 << 1;
        const ATLAS_UV = 1 << 2;
    }
}

impl Default for PushFlags {
    fn default() -> Self {
        PushFlags::TRANSFORM | PushFlags::ATLAS_UV
    }
}

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
    Simple,
    Lit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshVariant {
    Raw,
    Stitched,
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

#[allow(dead_code)]
struct PipelineEntry {
    layout: wgpu::PipelineLayout,
    opaque: wgpu::RenderPipeline,
    trans: wgpu::RenderPipeline,
    push_type: TypeId,
    push_size: usize,
    uniform_type: Option<TypeId>,
    uniform_size: usize,
    conf: ShaderConf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
struct BasePush {
    transform: [[f32; 4]; 4],
    uv_offset_scale: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct BasicPush {
    pub tint: [f32; 4],
    pub emission: [f32; 4],
}

impl Default for BasicPush {
    fn default() -> Self {
        BasicPush {
            tint: color::WHITE,
            emission: color::TRANS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteParams {
    pub pos: [f32; 2],
    pub pivot: Pivot,
    pub depth: Depth,
    pub pixelly: bool,
    pub tint: [f32; 4],
    pub emission: [f32; 4],
    pub cel: ([usize; 2], [usize; 2]),
    pub size: Size,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Size {
    #[default]
    Default,
    Set([f32; 2]),
    SetWidth(f32),
    SetHeight(f32),
    Scaled([f32; 2]),
}

impl Default for SpriteParams {
    fn default() -> Self {
        SpriteParams {
            pos: [0., 0.],
            pivot: Pivot::TL,
            depth: 0 * D,
            pixelly: true,
            tint: color::WHITE,
            emission: color::TRANS,
            cel: ([0, 0], [1, 1]),
            size: Size::Default,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct LitPush {
    pub tint: [f32; 4],
    pub light_dir: [f32; 4],
    pub light_col: [f32; 4],
}

impl Default for LitPush {
    fn default() -> Self {
        LitPush {
            tint: color::WHITE,
            light_dir: [0., -1., 0., 0.],
            light_col: color::WHITE,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct GlobalUniforms {
    v_mat: [[f32; 4]; 4],
    p_mat: [[f32; 4]; 4],
    vp_mat: [[f32; 4]; 4],
    view_vec: [f32; 4],
    params: [f32; 4],
    pixel_size: [f32; 2],
    canvas_size: [f32; 2],
    texel_size: [f32; 2],
    cursor_pos: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct TexturePage {
    index: u32,
    padding: [f32; 3],
}

struct GlyphCtx {
    draw_call_index: usize,
    count: u16,
    tint: [f32; 4],
    vp_matrix: Mat4<f32>,
}

fn push_constant_bytes<T: 'static>(t: &T) -> &[u8] {
    unsafe {
        let p = t as *const _ as *const u8;
        let size = std::mem::size_of::<T>();
        std::slice::from_raw_parts(p, size)
    }
}

fn push_constant_size(overhead: usize, conf: ShaderConf) -> usize {
    use std::mem::size_of;

    let mut size = overhead;
    for flag in conf.push_flags.iter() {
        size += match flag {
            PushFlags::TRANSFORM => size_of::<[[f32; 4]; 4]>(),
            PushFlags::MODEL_MATRIX => size_of::<[[f32; 4]; 4]>(),
            PushFlags::ATLAS_UV => size_of::<[f32; 4]>(),
            _ => unreachable!(),
        };
    }

    size
}

#[derive(Debug, Clone)]
struct CameraPass {
    canvas_config: CanvasConfig,
    view: Mat4<f32>,
    projection: Mat4<f32>,
    vp_matrix: Mat4<f32>,
    used: bool,
    global_bind_index: usize,
    opaque_call_range: Range<usize>,
    trans_call_range: Range<usize>,
}

#[derive(PartialEq)]
struct DrawCall {
    phase: i8,
    depth: Depth,
    shader_index: u8,
    binding_index: u8,
    uniforms_index: Option<u8>,
    index_range: Range<u32>,
    push_range: Range<usize>,
}

impl PartialOrd for DrawCall {
    fn partial_cmp(&self, other: &DrawCall) -> Option<std::cmp::Ordering> {
        Some(
            (
                self.phase,
                self.depth,
                self.shader_index,
                self.binding_index,
                self.uniforms_index,
            )
                .cmp(&(
                    other.phase,
                    other.depth,
                    other.shader_index,
                    other.binding_index,
                    other.uniforms_index,
                )),
        )
    }
}
impl Ord for DrawCall {
    fn cmp(&self, other: &DrawCall) -> std::cmp::Ordering {
        (
            self.phase,
            self.depth,
            self.shader_index,
            self.binding_index,
            self.uniforms_index,
        )
            .cmp(&(
                other.phase,
                other.depth,
                other.shader_index,
                other.binding_index,
                other.uniforms_index,
            ))
    }
}
impl Eq for DrawCall {}

#[allow(dead_code)]
pub struct DrawContext<ImageKey = BuiltinOnly, MeshKey = BuiltinOnly, ShaderKey = BuiltinOnly>
where
    ImageKey: Clone + Eq + Hash,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    canvas_config: CanvasConfig,
    texture_size: u32,
    texture_pages: u8,

    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    swapchain_format: wgpu::TextureFormat,
    surface_config: wgpu::SurfaceConfiguration,
    scale_factor: f64,
    surface_invalidated: Flag,
    depth_buffer: Option<wgpu::TextureView>,

    textures: (wgpu::Texture, wgpu::TextureView),
    samplers: [wgpu::Sampler; 2],

    globals_layout: wgpu::BindGroupLayout,
    sampling_layout: wgpu::BindGroupLayout,
    custom_layouts: HashMap<ShaderAssetKey<ShaderKey>, wgpu::BindGroupLayout>,
    global_bindings: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    sampling_bindings: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    custom_bindings: HashMap<ShaderAssetKey<ShaderKey>, Vec<(wgpu::Buffer, wgpu::BindGroup)>>,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    mesh_atlas: SubmeshAtlas<MeshAssetKey<MeshKey>, (MeshAssetKey<MeshKey>, MeshVariant), Vertex>,
    image_atlas: ImageArrayAtlas<'static, ImageAssetKey<ImageKey>>,
    image_atlas_images: Vec<RgbaImage>,
    built_in_font: crate::font::Font,
    font_atlas: FontAtlas,
    font_atlas_image: RgbaImage,
    shader_mapping: HashMap<ShaderAssetKey<ShaderKey>, u8>,
    pipelines: Vec<(ShaderAssetKey<ShaderKey>, PipelineEntry)>,

    time_prev_frame_submit: Option<Instant>,
    frame_stats: RenderStats,

    storage: FrameStorage<ShaderKey>,
    editor_context: EditorContext,
}

impl<ImageKey, MeshKey, ShaderKey> DrawContext<ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    pub async fn new(
        window: &Arc<Window>,
        canvas_config: CanvasConfig,
        texture_size: u32,
        texture_pages: u8,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let texture_pages = texture_pages + 1; // Font texture

        let backends = {
            #[cfg(web_platform)]
            {
                wgpu::Backends::GL
            }
            #[cfg(not(web_platform))]
            {
                wgpu::Backends::default()
            }
        };

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let surface = unsafe { instance.create_surface(Arc::clone(&window)).unwrap() };

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
                    required_features: wgpu::Features::PUSH_CONSTANTS,
                    required_limits: wgpu::Limits {
                        max_push_constant_size: 128,
                        ..wgpu::Limits::downlevel_webgl2_defaults()
                    }
                    .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::default(),
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
            // TODO: Can we do something to ensure the window _isn't_ zero
            // sized instead?
            width: std::cmp::max(100, size.width),
            height: std::cmp::max(100, size.height),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![swapchain_format],
            desired_maximum_frame_latency: 2,
        };
        let scale_factor = window.scale_factor();

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
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
            ],
        });

        let sampling_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<TexturePage>() as _
                        ),
                    },
                    count: None,
                },
            ],
        });

        let samplers: [_; SAMPLERS as usize] = [
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

        let textures = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: texture_size,
                height: texture_size,
                depth_or_array_layers: 4,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: None,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let textures_view = textures.create_view(&wgpu::TextureViewDescriptor::default());

        let global_bindings = (0..4)
            .map(|_| {
                let global_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: std::mem::size_of::<GlobalUniforms>() as _,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });

                let global_bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&textures_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: global_buffer.as_entire_binding(),
                        },
                    ],
                    layout: &globals_layout,
                    label: None,
                });

                (global_buffer, global_bindings)
            })
            .collect::<Vec<_>>();

        let num_bindings = texture_pages * SAMPLERS;
        let sampling_bindings = (0..num_bindings)
            .map(|i| {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: std::mem::size_of::<TexturePage>() as _,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });

                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(
                                &samplers[(i / texture_pages) as usize],
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: buffer.as_entire_binding(),
                        },
                    ],
                    layout: &sampling_layout,
                    label: None,
                });

                (buffer, bind_group)
            })
            .collect::<Vec<_>>();

        for sampler_index in 0..SAMPLERS {
            for index in 0..texture_pages {
                let locals = TexturePage {
                    index: index as _,
                    padding: [0.; 3],
                };

                let uniform_bytes = unsafe {
                    std::slice::from_raw_parts(
                        &locals as *const _ as *const u8,
                        std::mem::size_of::<TexturePage>(),
                    )
                };
                queue.write_buffer(
                    &sampling_bindings[(index + sampler_index * texture_pages) as usize].0,
                    0,
                    uniform_bytes,
                );
            }
        }

        let vertex_bytes = vec![0; MAX_VERTICES * std::mem::size_of::<Vertex>()];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &vertex_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
        });

        let index_bytes = vec![0; MAX_VERTICES * std::mem::size_of::<u16>()];
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
            sampling_layout,
            custom_layouts: HashMap::default(),
            global_bindings,
            sampling_bindings,
            custom_bindings: HashMap::default(),

            textures: (textures, textures_view),
            samplers,

            vertex_buffer,
            index_buffer,

            mesh_atlas: SubmeshAtlas::new(),
            image_atlas: ImageArrayAtlas::new([texture_size; 2], Some(3)),
            image_atlas_images: vec![
                RgbaImage::new(texture_size, texture_size);
                (texture_pages - 1) as usize
            ],
            built_in_font: crate::font::Font::load_default(),
            font_atlas: FontAtlas::with_size([texture_size; 2]),
            font_atlas_image: RgbaImage::new(texture_size, texture_size),
            shader_mapping: Default::default(),
            pipelines: Default::default(),

            time_prev_frame_submit: None,
            frame_stats: RenderStats::default(),

            storage: FrameStorage::new(),
            editor_context: EditorContext {
                shown: false,
                file: None,
                mode: EditMode::Default,
            },
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

        result.load_shader_internal::<BasicPush, ()>(
            AssetKey::Builtin(BuiltinShader::Basic),
            BUILTIN_SHADER,
            ShaderConf::default(),
        );
        result.load_shader_internal::<BasicPush, ()>(
            AssetKey::Builtin(BuiltinShader::YFlip),
            YFLIP_SHADER,
            ShaderConf {
                shader_flags: ShaderFlags::Y_FLIPPED,
                ..Default::default()
            },
        );
        result.load_shader_internal::<(), ()>(
            AssetKey::Builtin(BuiltinShader::Simple),
            SIMPLE_SHADER,
            ShaderConf::default(),
        );
        result.load_shader_internal::<LitPush, ()>(
            AssetKey::Builtin(BuiltinShader::Lit),
            LIT_SHADER,
            ShaderConf::default(),
        );

        result.load_mesh_internal(AssetKey::Builtin(BuiltinMesh::Quad), quad_mesh);
        result.load_mesh_internal(AssetKey::Builtin(BuiltinMesh::Sprite), sprite_mesh);

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
            } => {
                let max_dim = (std::u16::MAX / 4) as u32;
                if dims.width <= max_dim && dims.height <= max_dim {
                    self.resized((*dims).into())
                } else {
                    eprintln!("Resize exceeded max size of {}\n({:?})", max_dim, dims);
                }
            }
            Event::WindowEvent {
                event:
                    WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        inner_size_writer: _,
                    },
                ..
            } => self.scale_factor_changed(*scale_factor, ()),
            _ => (),
        }
    }

    pub fn resized(&mut self, new_inner_size: (u32, u32)) {
        self.surface_config.width = new_inner_size.0;
        self.surface_config.height = new_inner_size.1;
        self.surface_invalidated.set();
    }

    pub fn scale_factor_changed(&mut self, scale_factor: f64, _new_inner_size: ()) {
        // TODO: idk man
        // self.surface_config.width = new_inner_size.0;
        // self.surface_config.height = new_inner_size.1;
        self.scale_factor = scale_factor;
        self.surface_invalidated.set();
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    pub fn set_canvas_config(&mut self, canvas_config: CanvasConfig) {
        self.canvas_config = canvas_config;
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        self.surface_config.present_mode = match vsync {
            true => wgpu::PresentMode::AutoVsync,
            false => wgpu::PresentMode::AutoNoVsync,
        };
        self.surface_invalidated.set();
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

    pub fn frame_stats(&self) -> RenderStats {
        self.frame_stats.clone()
    }

    pub fn load_mesh(&mut self, key: MeshKey, mesh: Mesh<Vertex>) {
        let key = AssetKey::Key(key);
        self.load_mesh_internal(key, mesh);
    }

    fn load_mesh_stitched(&mut self, key: MeshAssetKey<MeshKey>, mesh: Mesh<Vertex>) {
        self.mesh_atlas.remove_mesh(&key);

        let Mesh {
            mut vertices,
            indices,
        } = mesh;

        let stitched_indices = crate::mesh::stitch_mesh(&mut vertices, &indices);

        let raw_submeshes = Submeshes {
            index_range: 0..indices.len() as u32,
            submeshes: vec![],
        };

        let stitched_submeshes = Submeshes {
            index_range: 0..stitched_indices.len() as u32,
            submeshes: vec![],
        };

        self.mesh_atlas.insert_vertices(key.clone(), vertices);
        self.mesh_atlas.insert_submeshes(
            key.clone(),
            (key.clone(), MeshVariant::Raw),
            indices,
            raw_submeshes,
        );
        self.mesh_atlas.insert_submeshes(
            key.clone(),
            (key.clone(), MeshVariant::Stitched),
            stitched_indices,
            stitched_submeshes,
        );
    }

    fn load_mesh_internal(&mut self, key: MeshAssetKey<MeshKey>, mesh: Mesh<Vertex>) {
        let Mesh { vertices, indices } = mesh;
        let submeshes = Submeshes {
            index_range: 0..indices.len() as u32,
            submeshes: vec![],
        };
        self.mesh_atlas.insert_vertices(key.clone(), vertices);
        self.mesh_atlas.insert_submeshes(
            key.clone(),
            (key.clone(), MeshVariant::Raw),
            indices,
            submeshes,
        );
    }

    pub fn load_image(&mut self, key: ImageKey, image: RgbaImage) {
        self.image_atlas.insert(AssetKey::Key(key), image);
    }

    fn load_image_internal(&mut self, key: BuiltinImage, image: RgbaImage) {
        self.image_atlas.insert(AssetKey::Builtin(key), image);
    }

    pub fn load_shader<P: 'static>(
        &mut self,
        key: ShaderKey,
        source: impl AsRef<str>,
        conf: ShaderConf,
    ) {
        self.load_shader_internal::<P, ()>(AssetKey::Key(key), source.as_ref(), conf)
    }

    pub fn load_shader_with_uniforms<P: 'static, U: 'static>(
        &mut self,
        key: ShaderKey,
        source: impl AsRef<str>,
        conf: ShaderConf,
    ) {
        self.load_shader_internal::<P, U>(AssetKey::Key(key), source.as_ref(), conf)
    }

    fn register_uniform_type<U: 'static>(&mut self, name: ShaderAssetKey<ShaderKey>) {
        if std::mem::size_of::<U>() != 0 {
            let key = name;

            if !self.custom_layouts.contains_key(&key) {
                let layout =
                    self.device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: None,
                            entries: &[wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::VERTEX
                                    | wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Uniform,
                                    has_dynamic_offset: false,
                                    min_binding_size: wgpu::BufferSize::new(
                                        std::mem::size_of::<U>() as _,
                                    ),
                                },
                                count: None,
                            }],
                        });
                self.custom_layouts.insert(key.clone(), layout);

                self.custom_bindings.insert(key, vec![]);
            }
        }
    }

    fn load_shader_internal<P: 'static, U: 'static>(
        &mut self,
        name: ShaderAssetKey<ShaderKey>,
        source: &str,
        conf: ShaderConf,
    ) {
        self.register_uniform_type::<U>(name.clone());
        let uniforms_layout = self.custom_layouts.get(&name);

        let bind_group_layouts = match uniforms_layout {
            None => vec![&self.globals_layout, &self.sampling_layout],
            Some(uniforms_layout) => {
                vec![&self.globals_layout, &self.sampling_layout, uniforms_layout]
            }
        };

        let push_type = TypeId::of::<P>();
        let push_size = std::mem::size_of::<P>();
        let total_push_size = push_constant_size(push_size, conf) as u32;
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &bind_group_layouts,
                push_constant_ranges: &[wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    range: 0..total_push_size,
                }],
            });

        let mut shader_source = SHADER_HEADER.to_string();
        shader_source.push_str(source);

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

        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let y_flipped = conf.shader_flags.contains(ShaderFlags::Y_FLIPPED);
        let back_face = conf.shader_flags.contains(ShaderFlags::BACK_FACE_ONLY);
        let front_face = match (y_flipped, back_face) {
            (false, false) => wgpu::FrontFace::Ccw,
            (true, false) => wgpu::FrontFace::Cw,
            (false, true) => wgpu::FrontFace::Cw,
            (true, true) => wgpu::FrontFace::Ccw,
        };

        let primitive = wgpu::PrimitiveState {
            front_face,
            cull_mode: Some(wgpu::Face::Back),
            ..wgpu::PrimitiveState::default()
        };

        let depth_compare = wgpu::CompareFunction::LessEqual;
        let should_add = conf.shader_flags.contains(ShaderFlags::BLEND_ADD);
        let add = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });

        let write_color = !conf.shader_flags.contains(ShaderFlags::NO_COLOR_WRITE);

        let opaque_fragment = wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: self.swapchain_format,
                blend: match (write_color, should_add) {
                    (true, true) => add,
                    _ => None,
                },
                write_mask: if write_color {
                    wgpu::ColorWrites::default()
                } else {
                    wgpu::ColorWrites::empty()
                },
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        };

        let trans_fragment = wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: self.swapchain_format,
                blend: match (write_color, should_add) {
                    (true, true) => add,
                    (true, _) => Some(wgpu::BlendState::ALPHA_BLENDING),
                    _ => None,
                },
                write_mask: if write_color {
                    wgpu::ColorWrites::default()
                } else {
                    wgpu::ColorWrites::empty()
                },
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        };

        let (stencil_shows, stencil_hides, stencil_add, stencil_sub) = (
            conf.shader_flags.contains(ShaderFlags::STENCIL_SHOWS),
            conf.shader_flags.contains(ShaderFlags::STENCIL_HIDES),
            conf.shader_flags.contains(ShaderFlags::STENCIL_ADD),
            conf.shader_flags.contains(ShaderFlags::STENCIL_SUB),
        );

        let stencil = wgpu::StencilState {
            read_mask: !0,
            write_mask: !0,
            back: wgpu::StencilFaceState::default(),
            front: wgpu::StencilFaceState {
                compare: match (stencil_shows, stencil_hides) {
                    (true, _) => wgpu::CompareFunction::NotEqual,
                    (false, true) => wgpu::CompareFunction::Equal,
                    _ => wgpu::CompareFunction::Always,
                },
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: match (stencil_add, stencil_sub) {
                    (true, _) => wgpu::StencilOperation::IncrementClamp,
                    (false, true) => wgpu::StencilOperation::DecrementClamp,
                    _ => wgpu::StencilOperation::Keep,
                },
            },
        };

        let bias = wgpu::DepthBiasState::default();

        let opaque_pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    buffers: &[vertex_buffer_layout.clone()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(opaque_fragment),
                primitive,
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24PlusStencil8,
                    depth_write_enabled: !conf.shader_flags.contains(ShaderFlags::NO_DEPTH_WRITE),
                    depth_compare,
                    stencil: stencil.clone(),
                    bias,
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let trans_pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    buffers: &[vertex_buffer_layout.clone()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(trans_fragment),
                primitive,
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24PlusStencil8,
                    depth_write_enabled: false,
                    depth_compare,
                    stencil,
                    bias,
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let shader_index = self.pipelines.len() as u8;
        self.shader_mapping.insert(name.clone(), shader_index);
        self.pipelines.push((name.clone(), PipelineEntry {
            layout: pipeline_layout,
            opaque: opaque_pipeline,
            trans: trans_pipeline,
            push_type,
            push_size,
            uniform_type: uniforms_layout.map(|_| TypeId::of::<U>()),
            uniform_size: std::mem::size_of::<U>(),
            conf,
        }));
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
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[wgpu::TextureFormat::Depth24PlusStencil8],
            });

            let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

            self.depth_buffer = Some(depth_view);
        }

        // TODO: Use updated range
        if let Some(_updated_range) = self.mesh_atlas.compile() {
            let mut indices = self.mesh_atlas.indices.clone();

            // TODO: Less hacky alignment fix
            if (indices.len() % 2) != 0 {
                indices.push(0);
            }

            let vertex_data = unsafe {
                std::slice::from_raw_parts(
                    self.mesh_atlas.vertices.as_ptr() as *const Vertex as *const u8,
                    self.mesh_atlas.vertices.len() * std::mem::size_of::<Vertex>(),
                )
            };
            let index_data = unsafe {
                std::slice::from_raw_parts(
                    indices.as_ptr() as *const u16 as *const u8,
                    indices.len() * std::mem::size_of::<u16>(),
                )
            };

            let vertex_offset = 0;
            let index_offset = 0;

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

        let canvas_config = self.canvas_config.clone();

        Renderer {
            context: self,
            clear_color,
            generic_params,
            cursor_pos,
            camera_pass: CameraPass {
                canvas_config,
                view: Mat4::identity(),
                projection: Mat4::identity(),
                vp_matrix: Mat4::identity(),
                used: false,
                global_bind_index: 0,
                opaque_call_range: 0..0,
                trans_call_range: 0..0,
            },
            time_created: Instant::now(),
            time_render_start: Instant::now(),
            time_render_end: Instant::now(),
            time_submit: Instant::now(),
            time_present: Instant::now(),
        }
    }
}

struct FrameStorage<ShaderKey> {
    pub camera_passes: Vec<CameraPass>,
    pub opaque_calls: Vec<DrawCall>,
    pub trans_calls: Vec<DrawCall>,
    pub glyph_buffer: HVec,
    pub push_constants: Vec<u8>,
    pub uniform_indices: HashMap<*const u8, u8>,
    pub uniform_bytes: HashMap<ShaderAssetKey<ShaderKey>, (usize, Vec<u8>)>,
}

impl<ShaderKey> FrameStorage<ShaderKey> {
    pub fn new() -> Self {
        FrameStorage {
            camera_passes: Vec::with_capacity(4),
            opaque_calls: Vec::with_capacity(128),
            trans_calls: Vec::with_capacity(128),
            glyph_buffer: HVec::with_capacity(128, 128),
            push_constants: vec![],
            uniform_indices: HashMap::default(),
            uniform_bytes: HashMap::default(),
        }
    }

    pub fn clear(&mut self) {
        self.camera_passes.clear();
        self.opaque_calls.clear();
        self.trans_calls.clear();
        self.glyph_buffer.clear();
        self.push_constants.clear();
        self.uniform_indices.clear();
        self.uniform_bytes.clear();
    }
}

pub struct Renderer<'context, ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    context: &'context mut DrawContext<ImageKey, MeshKey, ShaderKey>,
    clear_color: [f32; 4],
    generic_params: [f32; 4],
    cursor_pos: [f32; 2],
    camera_pass: CameraPass,
    time_created: Instant,
    time_render_start: Instant,
    time_render_end: Instant,
    time_submit: Instant,
    time_present: Instant,
}

impl<'context, ImageKey, MeshKey, ShaderKey> Renderer<'context, ImageKey, MeshKey, ShaderKey>
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
                let mut target = self.context.textures.0.as_image_copy();
                target.origin.z = i as u32;
                self.context.queue.write_texture(
                    target,
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
        self.time_render_start = Instant::now();

        if self.camera_pass.used || self.context.storage.camera_passes.is_empty() {
            self.end_camera_pass(None);
        }

        let mut stats = RenderStats {
            camera_passes: self.context.storage.camera_passes.len(),
            frame: self.context.frame_stats.frame + 1,
            pipeline_changes: 0,
            binding_changes: 0,
            uniform_changes: 0,
            opaque_objects: self.context.storage.opaque_calls.len(),
            transparent_objects: self.context.storage.trans_calls.len(),
            uniform_types: self.context.custom_bindings.len(),
            uniform_buffers: self
                .context
                .custom_bindings
                .values()
                .map(|v| v.len())
                .sum::<usize>(),
            opaque_bytes: self.context.storage.opaque_calls.len() * std::mem::size_of::<DrawCall>(),
            transparent_bytes: self.context.storage.trans_calls.len() * std::mem::size_of::<DrawCall>(),
            glyph_bytes: self.context.storage.glyph_buffer.bytes_len(),
            push_constant_bytes: self.context.storage.push_constants.len(),
            uniform_bytes: self
                .context.storage.uniform_bytes
                .values()
                .map(|(_, v)| v.len())
                .sum::<usize>(),
            total_cpu_bytes: 0,
            frame_time: Duration::default(),
            prepare_time: Duration::default(),
            render_time: Duration::default(),
            submit_time: Duration::default(),
            present_time: Duration::default(),
        };
        stats.total_cpu_bytes = stats.opaque_bytes
            + stats.transparent_bytes
            + stats.glyph_bytes
            + stats.push_constant_bytes
            + stats.uniform_bytes;

        self.prepare_glyphs();
        self.upload_uniforms();

        // Sort calls
        for camera_pass in &self.context.storage.camera_passes {
            // NOTE: We have to sort opaques, just not by depth.
            self.context.storage.opaque_calls[camera_pass.opaque_call_range.clone()].sort();
            self.context.storage.trans_calls[camera_pass.trans_call_range.clone()].sort();
        }

        let frame_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut commands = self
            .context
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Passes
        let cams = self.context.storage.camera_passes.len();
        println!("Camera passes {cams}");
        for (i, camera_pass) in self.context.storage.camera_passes.iter().enumerate() {
            let canvas_properties = camera_pass.canvas_config.canvas_properties(
                [
                    self.context.surface_config.width,
                    self.context.surface_config.height,
                ],
                self.context.scale_factor,
            );

            let [r, g, b, a] = self.clear_color;

            let clear_op = wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: r.into(),
                    g: g.into(),
                    b: b.into(),
                    a: a.into(),
                }),
                store: wgpu::StoreOp::Store,
            };

            let load_op = wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            };

            for opaque in [true, false] {
                let mut pass = match opaque {
                    true => commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &frame_view,
                            resolve_target: None,
                            ops: if i == 0 { clear_op } else { load_op },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: self.context.depth_buffer.as_ref().unwrap(),
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(0),
                                store: wgpu::StoreOp::Store,
                            }),
                        }),
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    }),
                    false => commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &frame_view,
                            resolve_target: None,
                            ops: load_op,
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: self.context.depth_buffer.as_ref().unwrap(),
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Discard,
                            }),
                            stencil_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Discard,
                            }),
                        }),
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    }),
                };

                pass.set_stencil_reference(0);

                let ([x, y], [w, h]) = canvas_properties.viewport_scissor_rect;
                pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
                pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0., 1.);
                pass.set_bind_group(
                    0,
                    &self.context.global_bindings[camera_pass.global_bind_index].1,
                    &[],
                );
                pass.set_vertex_buffer(0, self.context.vertex_buffer.slice(..));
                pass.set_index_buffer(
                    self.context.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );

                let mut active_pipeline = None;
                let mut active_binding = None;
                let mut active_uniforms = None;

                let calls = match opaque {
                    true => &self.context.storage.opaque_calls[camera_pass.opaque_call_range.clone()],
                    false => &self.context.storage.trans_calls[camera_pass.trans_call_range.clone()],
                };

                for call in calls {
                    if active_pipeline != Some(call.shader_index) {
                        stats.pipeline_changes += 1;
                        let entry = &self.context.pipelines[call.shader_index as usize].1;
                        let pipeline = match opaque {
                            true => &entry.opaque,
                            false => &entry.trans,
                        };
                        pass.set_pipeline(pipeline);
                        active_pipeline = Some(call.shader_index);
                        active_uniforms = None;
                    }

                    if active_binding != Some(call.binding_index) {
                        stats.binding_changes += 1;
                        pass.set_bind_group(
                            1,
                            &self.context.sampling_bindings[call.binding_index as usize].1,
                            &[],
                        );
                        active_binding = Some(call.binding_index);
                    }

                    if let Some(uniforms_index) = call.uniforms_index {
                        if active_uniforms != call.uniforms_index {
                            stats.uniform_changes += 1;

                            let uniform_type_key = &self.context.pipelines[call.shader_index as usize].0;

                            pass.set_bind_group(
                                2,
                                &self.context.custom_bindings[uniform_type_key]
                                    [uniforms_index as usize]
                                    .1,
                                &[],
                            );
                            active_uniforms = call.uniforms_index;
                        }
                    }

                    pass.set_push_constants(
                        wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        0,
                        &self.context.storage.push_constants[call.push_range.clone()],
                    );

                    pass.draw_indexed(call.index_range.clone(), 0, 0..1);
                }
            }
        }
        self.time_render_end = Instant::now();

        self.context.queue.submit(Some(commands.finish()));
        self.time_submit = Instant::now();

        frame.present();
        self.time_present = Instant::now();

        if let Some(prev_submit) = self.context.time_prev_frame_submit {
            stats.frame_time = self.time_submit - prev_submit;
        }
        self.context.time_prev_frame_submit = Some(self.time_submit);

        stats.prepare_time = self.time_render_start - self.time_created;
        stats.render_time = self.time_render_end - self.time_render_start;
        stats.submit_time = self.time_submit - self.time_render_end;
        stats.present_time = self.time_present - self.time_submit;
        self.context.frame_stats = stats;
    }

    fn prepare_glyphs(&mut self) {
        let mut glyph_iter = self.context.storage.glyph_buffer.iter();
        while let Some(ctx) = unsafe { glyph_iter.next_unchecked::<GlyphCtx>() } {
            for _ in 0..ctx.count {
                let glyph = unsafe { glyph_iter.next_unchecked::<Glyph>().unwrap() };
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
                let mut target = self.context.textures.0.as_image_copy();
                target.origin.z = self.context.texture_pages as u32 - 1;
                self.context.queue.write_texture(
                    target,
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
            std::mem::replace(&mut self.context.storage.glyph_buffer, HVec::with_capacity(128, 128)).into_iter();

        while let Some(ctx) = unsafe { glyph_buffer.next_unchecked::<GlyphCtx>() } {
            for i in 0..ctx.count {
                let glyph = unsafe { glyph_buffer.next_unchecked::<Glyph>().unwrap() };
                let draw_call_index = ctx.draw_call_index + i as usize;
                self.glyph_internal(draw_call_index, &glyph, ctx.tint, ctx.vp_matrix);
            }
        }
    }

    fn upload_uniforms(&mut self) {
        for (shader_key, (size, buffer)) in &self.context.storage.uniform_bytes {
            let bindings = self.context.custom_bindings.get_mut(shader_key).unwrap();
            for (i, bytes) in buffer.chunks(*size).enumerate() {
                if i >= bindings.len() {
                    let layout = &self.context.custom_layouts[shader_key];
                    let buffer = self.context.device.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: *size as _,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });

                    let binding =
                        self.context
                            .device
                            .create_bind_group(&wgpu::BindGroupDescriptor {
                                entries: &[wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: buffer.as_entire_binding(),
                                }],
                                layout: &layout,
                                label: None,
                            });

                    bindings.push((buffer, binding));
                }

                self.context.queue.write_buffer(&bindings[i].0, 0, bytes);
            }
        }
    }

    fn end_camera_pass(&mut self, canvas_config: Option<CanvasConfig>) {
        let current = &self.camera_pass;
        let canvas_config = canvas_config.unwrap_or_else(|| current.canvas_config.clone());
        let new_pass = CameraPass {
            canvas_config,
            view: current.view,
            projection: current.projection,
            vp_matrix: current.vp_matrix,
            used: false,
            global_bind_index: current.global_bind_index + 1,
            opaque_call_range: self.context.storage.opaque_calls.len()..self.context.storage.opaque_calls.len(),
            trans_call_range: self.context.storage.trans_calls.len()..self.context.storage.trans_calls.len(),
        };
        let mut current = std::mem::replace(&mut self.camera_pass, new_pass);

        self.update_globals(&current);

        current.opaque_call_range = current.opaque_call_range.start..(self.context.storage.opaque_calls.len());
        current.trans_call_range = current.trans_call_range.start..(self.context.storage.trans_calls.len());

        self.context.storage.camera_passes.push(current);
    }

    fn update_globals(&mut self, camera_pass: &CameraPass) {
        if camera_pass.global_bind_index >= self.context.global_bindings.len() {
            let global_buffer = self.context.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: std::mem::size_of::<GlobalUniforms>() as _,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let global_bindings =
                self.context
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(
                                    &self.context.textures.1,
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: global_buffer.as_entire_binding(),
                            },
                        ],
                        layout: &self.context.globals_layout,
                        label: None,
                    });

            self.context
                .global_bindings
                .push((global_buffer, global_bindings));
        }

        let canvas_properties = camera_pass.canvas_config.canvas_properties(
            [
                self.context.surface_config.width,
                self.context.surface_config.height,
            ],
            self.context.scale_factor,
        );

        let canvas_size = Vec2::from(canvas_properties.logical_canvas_size).as_f32();
        let globals = GlobalUniforms {
            v_mat: camera_pass.view.0,
            p_mat: camera_pass.projection.0,
            vp_mat: camera_pass.vp_matrix.0,
            view_vec: (camera_pass.view * vec4(0., 0., -1., 0.)).0,
            params: Vec4::from(self.generic_params).as_f32().0,
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

        self.context.queue.write_buffer(
            &self.context.global_bindings[camera_pass.global_bind_index].0,
            0,
            uniform_bytes,
        );
    }

    fn update_matrices(&mut self, view: Mat4<f32>, projection: Mat4<f32>) {
        if self.camera_pass.used {
            self.end_camera_pass(None);
        }

        self.camera_pass.view = view;
        self.camera_pass.projection = projection;
        self.camera_pass.vp_matrix = projection * view;
    }

    pub fn reset_projection(&mut self) {
        self.set_projection(Mat4::identity().0);
    }

    pub fn reset_view(&mut self) {
        self.set_view(Mat4::identity().0);
    }

    // TODO: This is a travesty.
    // Edit: idek what this does
    pub fn project_point(&self, point: [f32; 3]) -> [f32; 3] {
        let mut projected = self.camera_pass.vp_matrix * Vec3::from(point).extend(1.);
        projected /= projected.0[3];
        let mut projected = (projected.retract() + vec3(1., 1., 0.)) * 0.5;
        projected.0[1] = 1. - projected.0[1];

        let canvas_properties = self.context.canvas_config.canvas_properties(
            [self.context.surface_config.width, self.context.surface_config.height],
            self.context.scale_factor,
        );

        let canvas_size = Vec2::from(canvas_properties.logical_canvas_size).extend(1.);
        (projected * canvas_size).0
    }

    pub fn set_canvas_config(&mut self, canvas_config: CanvasConfig) {
        if self.camera_pass.used {
            self.end_camera_pass(Some(canvas_config.clone())); // TODO: hmmm...
        }
        self.camera_pass.canvas_config = canvas_config;
    }

    pub fn set_projection(&mut self, matrix: [[f32; 4]; 4]) {
        self.update_matrices(self.camera_pass.view, Mat4::new(matrix));
    }

    pub fn set_view(&mut self, matrix: [[f32; 4]; 4]) {
        self.update_matrices(Mat4::new(matrix), self.camera_pass.projection);
    }

    pub fn set_view_within_pass(&mut self, matrix: [[f32; 4]; 4]) {
        self.camera_pass.view = Mat4::new(matrix);
        self.camera_pass.projection = self.camera_pass.projection;
        self.camera_pass.vp_matrix = self.camera_pass.projection * self.camera_pass.view;
    }

    pub fn modify_view(&mut self, matrix: [[f32; 4]; 4]) {
        self.update_matrices(
            Mat4::new(matrix) * self.camera_pass.view,
            self.camera_pass.projection,
        );
    }

    pub fn ortho_2d(&mut self) {
        let canvas_properties = self.camera_pass.canvas_config.canvas_properties(
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

        self.update_matrices(
            Mat4::translation([-half_width, -half_height, 0.]),
            matrix::ortho_projection(aspect, half_height, -1., 1.),
        );
    }

    pub fn perspective_3d(&mut self, fov: f32) {
        let canvas_properties = self.camera_pass.canvas_config.canvas_properties(
            [
                self.context.surface_config.width,
                self.context.surface_config.height,
            ],
            self.context.scale_factor,
        );

        let [cx, cy] = canvas_properties.logical_canvas_size;
        let aspect = cx as f32 / cy as f32;

        self.update_matrices(
            Mat4::identity(),
            matrix::perspective_projection(aspect, fov, 0.1, 1000.),
        );
    }

    pub fn stored_sprite<K: Into<ImageAssetKey<ImageKey>>>(
        &mut self,
        image: K,
        params: SpriteParams,
    ) -> Frame {
        let image = image.into();
        let (_page, region) = self.context.image_atlas.fetch(&image).unwrap();
        let [x, y] = params.pos;
        let [w, h] = region.size();
        let [cels_x, cels_y] = params.cel.1;
        let [def_w, def_h] = [w as f32 / cels_x as f32, h as f32 / cels_y as f32];

        let [w, h] = match params.size {
            Size::Default => [def_w, def_h],
            Size::Set(size) => size,
            Size::SetWidth(w) => [w, (w / def_w) * def_h],
            Size::SetHeight(h) => [(h / def_h) * def_w, h],
            Size::Scaled([scale_w, scale_h]) => [def_w * scale_w, def_h * scale_h],
        };

        let pw = params.pivot.0[0] * w;
        let ph = params.pivot.0[1] * h;

        self.stored_draw_internal(
            BuiltinShader::YFlip,
            image,
            params.cel,
            BuiltinMesh::Sprite,
            MeshVariant::Raw,
            (Mat4::translation([x - pw, y - ph, 0.]) * Mat4::scale([w, h, 1., 1.])).0,
            BasicPush {
                tint: params.tint,
                emission: params.emission,
            },
            &(),
            params.pixelly,
            Some(params.depth),
        );
        Anchor {
            pos: [x, y],
            pivot: params.pivot,
        }
        .frame([w, h])
    }

    fn glyph_internal(
        &mut self,
        draw_call_index: usize,
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

            let base_push = BasePush {
                transform: (vp_matrix
                    * Mat4::translation([x, y, 0.])
                    * Mat4::scale([w, h, 1., 1.]))
                .0,
                uv_offset_scale: [u, v, us, vs],
            };
            let push = BasicPush {
                tint,
                emission: color::TRANS,
            };
            let start_index = self.context.storage.push_constants.len();
            let base_push_bytes = push_constant_bytes(&base_push);
            self.context.storage.push_constants.extend_from_slice(base_push_bytes);
            let push_bytes = push_constant_bytes(&push);
            self.context.storage.push_constants.extend_from_slice(push_bytes);
            let end_index = self.context.storage.push_constants.len();

            self.context.storage.trans_calls[draw_call_index].push_range = start_index..end_index;
        }
    }


    fn draw_tinted_overlay(&mut self) {
        self.stored_sprite(BuiltinImage::White, SpriteParams {
            pos: [-40., -40.],
            pixelly: false,
            tint: [0.125, 0., 0.5, 0.75],
            size: Size::Set([4000., 4000.]),
            pivot: Pivot::TL,
            .. Default::default()
        });
    }

    pub fn draw_console(&mut self, console: &dbgcmd::Console) {
        if console.shown() {
            self.ortho_2d();
            self.draw_tinted_overlay();

            let (cur, glyphs) = self.context.built_in_font.layout_line_cur(console.entry(), [4., 4.], self.context.scale_factor, None);
            let glyphs_2 = self.context.built_in_font.layout_line("|", cur.into(), self.context.scale_factor, None);
            self.glyphs(&glyphs, [0., 0.], [0., 1., 0., 1.], D, false);
            self.glyphs(&glyphs_2, [0., 0.], [0., 0.5, 0., 1.], D, false);

            for (i, line) in console.history().take(32).enumerate() {
                let glyphs = self.context.built_in_font.layout_line(line, [8., 4. + (i as f32 + 1.) * 16.], self.context.scale_factor, None);
                self.glyphs(&glyphs, [0., 0.], [0., 0.5, 0., 1.], D, false);
            }
        }
    }

    pub fn draw_editor<P: AsRef<std::path::Path>>(&mut self, path: P) {
        use serde_yaml::Value;
        self.ortho_2d();
        self.draw_tinted_overlay();

        let source = std::fs::read_to_string(path.as_ref()).unwrap();
        let object: Value = serde_yaml::from_str(&source).unwrap();

        if let Value::Mapping(mapping) = object {
            let dx = 0.;
            let mut dy = 0.;
            self.draw_editor_mapping(dx, &mut dy, &mapping);
        } else {
            eprintln!("Cannot render non-map YAML file.");
        }
    }

    fn draw_editor_mapping(&mut self, dx: f32, dy: &mut f32, mapping: &serde_yaml::Mapping) {
        use serde_yaml::Value;

        for (key, value) in mapping.iter() {
            if let Value::String(s) = key {
                let glyphs = self.context.built_in_font.layout_line(s, [4. + dx, 4. + *dy], self.context.scale_factor, None);
                self.glyphs(&glyphs, [0., 0.], [0., 1., 0., 1.], D, false);
                *dy += 16.;
            }
            if let Value::Mapping(m) = value {
                self.draw_editor_mapping(dx + 16., dy, m);
            }
        }
    }

    pub fn glyphs<'g, I>(
        &mut self,
        glyphs: I,
        offset: [f32; 2],
        tint: [f32; 4],
        depth: Depth,
        pixelly: bool,
    ) where
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
        let count = glyphs.len() as u16;

        let shader_index = self.context.shader_mapping[&BuiltinShader::YFlip.into()];
        let shader_entry = &self.context.pipelines[shader_index as usize].1;
        let mesh_index = self
            .context
            .mesh_atlas
            .fetch_submesh(&(BuiltinMesh::Sprite.into(), MeshVariant::Raw), None)
            .unwrap();

        let draw_call_index = self.context.storage.trans_calls.len();
        self.context.storage.glyph_buffer.push::<GlyphCtx>(GlyphCtx {
            draw_call_index,
            count,
            tint,
            vp_matrix: self.camera_pass.vp_matrix,
        });
        let px = if pixelly {
            self.context.texture_pages
        } else {
            0
        };
        for glyph in glyphs {
            self.context.storage.glyph_buffer.push(glyph);

            // Placeholder draw call
            self.context.storage.trans_calls.push(DrawCall {
                phase: shader_entry.conf.phase,
                depth,
                shader_index,
                binding_index: self.context.texture_pages - 1 + px,
                uniforms_index: None,
                index_range: mesh_index.index_range.clone(),
                push_range: 0..0,
            });
        }
        self.camera_pass.used = true;
    }

    pub fn glyphs_partial<'g, I, F: Fn(char) -> f64>(
        &mut self,
        glyphs: I,
        offset: [f32; 2],
        tint: [f32; 4],
        depth: Depth,
        pixelly: bool,
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

        let shader_index = self.context.shader_mapping[&BuiltinShader::YFlip.into()];
        let shader_entry = &self.context.pipelines[shader_index as usize].1;
        let mesh_index = self
            .context
            .mesh_atlas
            .fetch_submesh(&(BuiltinMesh::Sprite.into(), MeshVariant::Raw), None)
            .unwrap();

        let draw_call_index = self.context.storage.trans_calls.len();
        self.context.storage.glyph_buffer.push(GlyphCtx {
            draw_call_index,
            count: to_render.len() as u16,
            tint,
            vp_matrix: self.camera_pass.vp_matrix,
        });

        let px = if pixelly {
            self.context.texture_pages
        } else {
            0
        };

        for glyph in to_render {
            self.context.storage.glyph_buffer.push(glyph);

            // Placeholder draw call
            self.context.storage.trans_calls.push(DrawCall {
                phase: shader_entry.conf.phase,
                depth,
                shader_index,
                binding_index: self.context.texture_pages - 1 + px,
                uniforms_index: None,
                index_range: mesh_index.index_range.clone(),
                push_range: 0..0,
            });
        }
        self.camera_pass.used = true;

        if done {
            (budget, None)
        } else {
            (0., Some(drawn))
        }
    }

    fn queue_draw_call<P: 'static, U: 'static>(
        &mut self,
        shader: ShaderAssetKey<ShaderKey>,
        page: u8,
        sampler_index: u8,
        mesh: MeshAssetKey<MeshKey>,
        mesh_variant: MeshVariant,
        mut base_push: BasePush,
        push: P,
        uniforms: &U,
        transparent_depth: Option<Depth>,
    ) {
        let shader_index = self.context.shader_mapping[&shader];
        let shader_entry = &self.context.pipelines[shader_index as usize].1;
        let phase = shader_entry.conf.phase;

        assert_eq!(
            std::mem::size_of::<P>(),
            shader_entry.push_size,
            "Push constant size doesn't match shader with index `{}`",
            shader_index
        );

        let uniform_size = std::mem::size_of::<U>();
        assert_eq!(
            uniform_size, shader_entry.uniform_size,
            "Uniform size doesn't match shader with index `{}`",
            shader_index
        );

        let binding_index = page + sampler_index * self.context.texture_pages;

        let mesh_index = self
            .context
            .mesh_atlas
            .fetch_submesh(&(mesh, mesh_variant), None)
            .unwrap();

        let model_matrix = base_push.transform;
        base_push.transform = (self.camera_pass.vp_matrix * Mat4::from(model_matrix)).0;

        let start_index = self.context.storage.push_constants.len();

        if shader_entry.conf.push_flags.contains(PushFlags::TRANSFORM) {
            let push_bytes = push_constant_bytes(&base_push.transform);
            self.context.storage.push_constants.extend_from_slice(push_bytes);
        }
        if shader_entry
            .conf
            .push_flags
            .contains(PushFlags::MODEL_MATRIX)
        {
            let push_bytes = push_constant_bytes(&model_matrix);
            self.context.storage.push_constants.extend_from_slice(push_bytes);
        }
        if shader_entry.conf.push_flags.contains(PushFlags::ATLAS_UV) {
            let push_bytes = push_constant_bytes(&base_push.uv_offset_scale);
            self.context.storage.push_constants.extend_from_slice(push_bytes);
        }

        let push_bytes = push_constant_bytes(&push);
        self.context.storage.push_constants.extend_from_slice(push_bytes);

        let end_index = self.context.storage.push_constants.len();

        let uniforms_index = (uniform_size > 0).then(|| {
            let ptr = uniforms as *const _ as *const u8;
            if let Some(index) = self.context.storage.uniform_indices.get(&ptr) {
                *index
            } else {
                let bytes = unsafe { std::slice::from_raw_parts(ptr, uniform_size) };
                let buffer = self
                    .context.storage.uniform_bytes
                    .entry(shader.clone())
                    .or_insert((uniform_size, vec![]));
                let buffer = &mut buffer.1;
                let index = (buffer.len() / uniform_size) as u8;
                buffer.extend_from_slice(bytes);

                self.context.storage.uniform_indices.insert(ptr, index);

                index
            }
        });

        if let Some(depth) = transparent_depth {
            self.context.storage.trans_calls.push(DrawCall {
                phase,
                depth,
                shader_index,
                binding_index,
                uniforms_index,
                index_range: mesh_index.index_range,
                push_range: start_index..end_index,
            });
        } else {
            self.context.storage.opaque_calls.push(DrawCall {
                phase,
                depth: 0 * D,
                shader_index,
                binding_index,
                uniforms_index,
                index_range: mesh_index.index_range,
                push_range: start_index..end_index,
            });
        }

        self.camera_pass.used = true;
    }

    pub fn stored_draw<P: 'static, U: 'static, I, M, S>(
        &mut self,
        shader: S,
        image: I,
        mesh: M,
        transform: [[f32; 4]; 4],
        push: P,
        uniforms: &U,
        pixel_texture: bool,
        transparent_depth: Option<Depth>,
    ) where
        I: Into<ImageAssetKey<ImageKey>>,
        M: Into<MeshAssetKey<MeshKey>>,
        S: Into<ShaderAssetKey<ShaderKey>>,
    {
        self.stored_draw_internal(
            shader,
            image,
            ([0, 0], [1, 1]),
            mesh,
            MeshVariant::Raw,
            transform,
            push,
            uniforms,
            pixel_texture,
            transparent_depth,
        );
    }

    fn stored_draw_internal<P: 'static, U: 'static, I, M, S>(
        &mut self,
        shader: S,
        image: I,
        image_region: ([usize; 2], [usize; 2]),
        mesh: M,
        mesh_variant: MeshVariant,
        transform: [[f32; 4]; 4],
        push: P,
        uniforms: &U,
        pixel_texture: bool,
        transparent_depth: Option<Depth>,
    ) where
        I: Into<ImageAssetKey<ImageKey>>,
        M: Into<MeshAssetKey<MeshKey>>,
        S: Into<ShaderAssetKey<ShaderKey>>,
    {
        let image = image.into();
        let sampler_index = if pixel_texture { 1 } else { 0 };
        let (page, region) = self.context.image_atlas.fetch(&image).unwrap();

        let ([sprite_x, sprite_y], [sheet_w, sheet_h]) = image_region;
        let (u_scale, v_scale) = (
            region.uv.1[0] / sheet_w as f32,
            region.uv.1[1] / sheet_h as f32,
        );

        let base_push = BasePush {
            transform,
            uv_offset_scale: [
                region.uv.0[0] + sprite_x as f32 * u_scale,
                region.uv.0[1] + sprite_y as f32 * v_scale,
                u_scale,
                v_scale,
            ],
        };

        self.queue_draw_call(
            shader.into(),
            page as u8,
            sampler_index,
            mesh.into(),
            mesh_variant,
            base_push,
            push,
            uniforms,
            transparent_depth,
        );
    }

    pub fn stored_image_region<I>(&self, image: I) -> Option<(usize, crate::draw::Region)>
    where
        I: Into<ImageAssetKey<ImageKey>>,
    {
        self.context
            .image_atlas
            .fetch(&image.into())
            .map(|(i, r)| (i, r.clone()))
    }
}

impl<'context, ImageKey, MeshKey, ShaderKey> Renderer<'context, ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash + glace::Asset<Value = image::RgbaImage>,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    pub fn sprite<I>(&mut self, image: I, params: SpriteParams) -> Frame
    where
        I: Into<ImageAssetKey<ImageKey>>,
    {
        let image = image.into();
        if let AssetKey::Key(key) = &image {
            if self.context.image_atlas.fetch(&image).is_none() {
                self.context.load_image(key.clone(), key.value());
            }
        }
        self.stored_sprite(image, params)
    }

    pub fn image_region<I>(&mut self, image: I) -> Option<(usize, crate::draw::Region)>
    where
        I: Into<ImageAssetKey<ImageKey>>,
    {
        let image = image.into();
        if let AssetKey::Key(key) = &image {
            if self.context.image_atlas.fetch(&image).is_none() {
                self.context.load_image(key.clone(), key.value());
            }
        }
        self.stored_image_region(image)
    }
}

impl<'context, ImageKey, MeshKey, ShaderKey> Renderer<'context, ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash + glace::Asset<Value = image::RgbaImage>,
    MeshKey: Clone + Eq + Hash + glace::Asset<Value = Cow<'static, [u8]>>,
    ShaderKey: Clone + Eq + Hash,
{
    pub fn draw<P: 'static, U: 'static, I, M, S>(
        &mut self,
        shader: S,
        image: I,
        mesh: M,
        transform: [[f32; 4]; 4],
        push: P,
        uniforms: &U,
        pixel_texture: bool,
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
            if self
                .context
                .mesh_atlas
                .fetch_submesh(&(mesh.clone(), MeshVariant::Raw), None)
                .is_none()
            {
                self.context
                    .load_mesh(key.clone(), crate::mesh::load_glb(&key.value()).unwrap());
            }
        }
        self.stored_draw_internal(
            shader,
            image,
            ([0, 0], [1, 1]),
            mesh,
            MeshVariant::Raw,
            transform,
            push,
            uniforms,
            pixel_texture,
            transparent_depth,
        )
    }

    pub fn draw_stitched<P: 'static, U: 'static, I, M, S>(
        &mut self,
        shader: S,
        image: I,
        mesh: M,
        transform: [[f32; 4]; 4],
        push: P,
        uniforms: &U,
        pixel_texture: bool,
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
            if self
                .context
                .mesh_atlas
                .fetch_submesh(&(mesh.clone(), MeshVariant::Stitched), None)
                .is_none()
            {
                self.context
                    .load_mesh_stitched(key.into(), crate::mesh::load_glb(&key.value()).unwrap());
            }
        }
        self.stored_draw_internal(
            shader,
            image,
            ([0, 0], [1, 1]),
            mesh,
            MeshVariant::Stitched,
            transform,
            push,
            uniforms,
            pixel_texture,
            transparent_depth,
        )
    }
}

impl<'context, ImageKey, MeshKey, ShaderKey> Drop for Renderer<'context, ImageKey, MeshKey, ShaderKey>
where
    ImageKey: Clone + Eq + Hash,
    MeshKey: Clone + Eq + Hash,
    ShaderKey: Clone + Eq + Hash,
{
    fn drop(&mut self) {
        self.render();
    }
}

struct EditorContext {
    shown: bool,
    file: Option<EditingFile>,
    mode: EditMode,
}

struct EditingFile {
    path: PathBuf,
    modified: SystemTime,
    value: serde_yaml::Mapping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditMode {
    Default,
    HSV,
}
