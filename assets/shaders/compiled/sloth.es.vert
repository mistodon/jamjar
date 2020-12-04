#version 300 es

const vec2 _19[12] = vec2[](vec2(-1.0), vec2(-1.0, 1.0), vec2(1.0), vec2(-1.0), vec2(1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(-1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0));
const vec2 _25[12] = vec2[](vec2(0.0), vec2(0.0, 1.0), vec2(1.0), vec2(0.0), vec2(1.0), vec2(1.0, 0.0), vec2(0.0), vec2(0.0, 1.0), vec2(1.0), vec2(0.0), vec2(1.0), vec2(1.0, 0.0));

out vec2 v_uv;

void main()
{
    v_uv = _25[gl_VertexID];
    gl_Position = vec4(_19[gl_VertexID], 0.0, 1.0);
}

