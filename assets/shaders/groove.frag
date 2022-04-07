#version 450
#extension GL_ARB_separate_shader_objects : enable

layout(location = 0) in vec4 vcolor;
layout(location = 1) in vec4 vuv;

layout(location = 0) out vec4 target;

layout(set = 0, binding = 0) uniform texture2D color_map;
layout(set = 0, binding = 1) uniform sampler color_sampler;

void main() {
    vec4 tex = vcolor * texture(sampler2D(color_map, color_sampler), vuv.xy);

    float value = tex.r + tex.b;
    float blackLine = step(0.25, value);
    float dimLine = step(0.75, value);
    float lightLine = step(1.5, value);
    float swatchIndex = blackLine + dimLine + lightLine; // 0 to 3

    // Assume palette to be in top left of texture
    // Texture is 4096 squared. Palette swatch is 8x8
    // width of a square: 0.001953125
    // half of that: 0.0009765625
    float paletteIndex = vuv.z;
    float split = vuv.w;
    vec2 paletteUv = vec2(split + 0.001953125 * swatchIndex + 0.0009765625, 0.001953125 * paletteIndex + 0.0009765625);

    vec4 finalcolor = texture(sampler2D(color_map, color_sampler), paletteUv);
    target = vec4(finalcolor.rgb, tex.a);
}
