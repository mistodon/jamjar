#version 300 es
precision mediump float;
precision highp int;

uniform highp sampler2D SPIRV_Cross_Combinedcolor_mapcolor_sampler;

layout(location = 0) out highp vec4 target;
in highp vec2 v_uv;

void main()
{
    target = texture(SPIRV_Cross_Combinedcolor_mapcolor_sampler, v_uv);
}

