#version 300 es

out vec4 vcolor;
layout(location = 0) in vec4 tint;
out vec2 vuv;
layout(location = 1) in vec2 uv;
layout(location = 2) in vec3 offset;

void main()
{
    vcolor = tint;
    vuv = uv;
    gl_Position = vec4(offset, 1.0);
}

