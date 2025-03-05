struct Push {
    transform: mat4x4<f32>,
    uv_offset_scale: vec4<f32>,
    tint: vec4<f32>,
    light_dir: vec4<f32>,
    light_col: vec4<f32>,
};

var<push_constant> push: Push;

@vertex
fn vertex_main(vertex: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = push.transform * vertex.position;
    output.normal = normalize(push.transform * vertex.normal).xyz;
    output.uv = vertex.uv.xy * (push.uv_offset_scale.zw) + push.uv_offset_scale.xy;
    output.color = vertex.color;
    return output;
}

@fragment
fn fragment_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    var base_color = textureSample(
        textures,
        textureSampler,
        vertex.uv,
        texture_page.index,
    );

    var light_dot = max(0.0, dot(-push.light_dir.xyz, vertex.normal));
    var lighting = vec4(push.light_col.rgb * light_dot, 1.0);

    return (base_color * vertex.color * push.tint * lighting);
}
