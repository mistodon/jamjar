#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec4 vcolor;
layout(location = 1) in vec2 vuv;

layout(location = 0) out vec4 target;

layout(set = 0, binding = 0) uniform texture2D color_map;
layout(set = 0, binding = 1) uniform sampler color_sampler;

layout(push_constant) uniform PushConstants {
    vec4 white;
    vec4 light;
    vec4 dim;
    vec4 black;
} push_constants;

void main() {
    vec4 tex = vcolor * texture(sampler2D(color_map, color_sampler), vuv);
    float value = tex.r + tex.b;
    float blackLine = step(0.25, value);
    float dimLine = step(0.75, value);
    float lightLine = step(1.5, value);
    vec4 finalcolor =
        (1.0 - blackLine) * push_constants.black
        + lightLine * push_constants.white
        + (blackLine * (1.0 - dimLine)) * push_constants.dim
        + (dimLine * (1.0 - lightLine)) * push_constants.light;
    target = vec4(finalcolor.rgb, tex.a);
}
