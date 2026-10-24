@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> scale: vec4<f32>;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vertex(@builtin(vertex_index) i: u32) -> Vertex {
    let p = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    var out: Vertex; out.position=vec4<f32>(p[i],0.0,1.0);
    out.uv=p[i]*vec2<f32>(0.5,-0.5)+vec2<f32>(0.5); return out;
}
@fragment fn tile(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(image, vec2<i32>(p.xy) % vec2<i32>(256), 0);
}
@fragment fn display(input: Vertex) -> @location(0) vec4<f32> {
    let size=vec2<f32>(textureDimensions(image));
    let xy=vec2<i32>(min(floor((input.uv*scale.xy+scale.zw)*size),size-vec2<f32>(1.0)));
    return textureLoad(image,xy,0);
}
