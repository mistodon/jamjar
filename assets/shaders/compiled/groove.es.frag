#version 300 es
precision mediump float;
precision highp int;

uniform highp sampler2D SPIRV_Cross_Combinedcolor_mapcolor_sampler;

in highp vec4 vcolor;
in highp vec2 vuv;
layout(location = 0) out highp vec4 target;

void main()
{
    highp vec4 tex = vcolor * texture(SPIRV_Cross_Combinedcolor_mapcolor_sampler, vuv);
    target = tex;
}

