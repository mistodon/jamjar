@vertex
fn vertex_main(vertex: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = push.transform * vertex.position;
    output.normal = normalize(push.transform * vertex.normal).xyz;
    output.uv = vertex.uv.xy;
    output.color = vec4(1.0, 1.0, 1.0, 1.0);
    return output;
}

@fragment
fn fragment_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    return push.emission;
}
