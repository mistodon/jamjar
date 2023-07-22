struct VertexInput {
    @location(0) position: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) uv: vec4<f32>,
    @location(3) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) custom_a: vec4<f32>,
    @location(4) custom_b: vec4<f32>,
};

struct TexturePage {
    index: u32,
    padding_0: f32,
    padding_1: f32,
    padding_2: f32,
};

struct GlobalUniforms {
    view_vec: vec4<f32>,
    params: vec4<f32>,
    pixel_size: vec2<f32>,
    canvas_size: vec2<f32>,
    texel_size: vec2<f32>,
    cursor_pos: vec2<f32>,
};

@group(0)
@binding(0)
var textures: texture_2d_array<f32>;

@group(0)
@binding(1)
var<uniform> global_uniforms: GlobalUniforms;

@group(1)
@binding(0)
var textureSampler: sampler;

@group(1)
@binding(1)
var<uniform> texture_page: TexturePage;

