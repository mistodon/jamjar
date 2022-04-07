#version 300 es
precision mediump float;
precision highp int;

uniform highp sampler2D SPIRV_Cross_Combinedcolor_mapcolor_sampler;

in highp vec4 vcolor;
in highp vec4 vuv;
layout(location = 0) out highp vec4 target;

void main()
{
    highp vec4 tex = vcolor * texture(SPIRV_Cross_Combinedcolor_mapcolor_sampler, vuv.xy);
    highp float value = tex.x + tex.z;
    highp float blackLine = step(0.25, value);
    highp float dimLine = step(0.75, value);
    highp float lightLine = step(1.5, value);
    highp float swatchIndex = (blackLine + dimLine) + lightLine;
    highp float paletteIndex = vuv.z;
    highp float split = vuv.w;
    highp vec2 paletteUv = vec2((split + (0.001953125 * swatchIndex)) + 0.0009765625, (0.001953125 * paletteIndex) + 0.0009765625);
    highp vec4 finalcolor = texture(SPIRV_Cross_Combinedcolor_mapcolor_sampler, paletteUv);
    target = vec4(finalcolor.xyz, tex.w);
}

