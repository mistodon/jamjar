#version 300 es
precision mediump float;
precision highp int;

struct PushConstants
{
    highp vec4 white;
    highp vec4 light;
    highp vec4 dim;
    highp vec4 black;
};

uniform PushConstants push_constants;

uniform highp sampler2D SPIRV_Cross_Combinedcolor_mapcolor_sampler;

in highp vec4 vcolor;
in highp vec2 vuv;
layout(location = 0) out highp vec4 target;

void main()
{
    highp vec4 tex = vcolor * texture(SPIRV_Cross_Combinedcolor_mapcolor_sampler, vuv);
    highp float value = tex.x + tex.z;
    highp float blackLine = step(0.25, value);
    highp float dimLine = step(0.75, value);
    highp float lightLine = step(1.5, value);
    highp vec4 finalcolor = (((push_constants.black * (1.0 - blackLine)) + (push_constants.white * lightLine)) + (push_constants.dim * (blackLine * (1.0 - dimLine)))) + (push_constants.light * (dimLine * (1.0 - lightLine)));
    target = vec4(finalcolor.xyz, tex.w);
}

