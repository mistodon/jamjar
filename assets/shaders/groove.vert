#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec4 tint;
layout(location = 1) in vec4 uv;
layout(location = 2) in vec3 offset;

layout(location = 0) out vec4 vcolor;
layout(location = 1) out vec4 vuv;

void main() {
    vcolor = tint;
    vuv = uv;
    gl_Position = vec4(offset, 1.0);
}
