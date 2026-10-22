struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@group(0) @binding(0) var test_texture: texture_2d<f32>;
@group(0) @binding(1) var nearest_sampler: sampler;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0)
    );
    var out: VertexOutput;
    let p = positions[index];
    out.position = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
    return out;
}
@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(test_texture, nearest_sampler, input.uv);
}
