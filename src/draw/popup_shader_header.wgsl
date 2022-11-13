struct Push {
    transform: mat4x4<f32>,
    uv_offset_scale: vec4<f32>,
    tint: vec4<f32>,
    emission: vec4<f32>,
    color_a: vec4<f32>,
    color_b: vec4<f32>,
};

struct GlobalUniforms {
    view_vec: vec4<f32>,
    times: vec4<f32>,
    pixel_size: vec2<f32>,
    canvas_size: vec2<f32>,
    texel_size: vec2<f32>,
    cursor_pos: vec2<f32>,
};

struct LocalUniforms {
    texture_index: u32,
    sampler_index: u32,
    padding_0: f32,
    padding_1: f32,
};

var<push_constant> push: Push;

@group(0)
@binding(0)
var<uniform> global_uniforms: GlobalUniforms;

@group(0)
@binding(1)
var textures: binding_array<texture_2d<f32>>;

@group(0)
@binding(2)
var samplers: binding_array<sampler>;

@group(1)
@binding(0)
var<uniform> uniforms: LocalUniforms;

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
};
