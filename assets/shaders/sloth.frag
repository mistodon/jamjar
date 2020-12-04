#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec2 v_uv;

layout(location = 0) out vec4 target;

layout(set = 0, binding = 0) uniform texture2D color_map;
layout(set = 0, binding = 1) uniform sampler color_sampler;

void main() {
    target = texture(sampler2D(color_map, color_sampler), v_uv);
}

