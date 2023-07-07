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
        texturesTest,
        textureSampler,
        vertex.uv,
        uniforms.texture_index
    );

    return (base_color * vertex.color * push.tint) + push.emission;
}
