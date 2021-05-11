#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec4 vcolor;
layout(location = 1) in vec2 vuv;

layout(location = 0) out vec4 target;

layout(set = 0, binding = 0) uniform texture2D color_map;
layout(set = 0, binding = 1) uniform sampler color_sampler;

void main() {
    vec4 tex = vcolor * texture(sampler2D(color_map, color_sampler), vuv);
    target = tex;
}
