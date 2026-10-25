struct View {
    screen: vec2<f32>, doc: vec2<f32>,
    pan: vec2<f32>, zoom: f32, rotation: f32,
    flip: f32, grid: f32, has_document: f32, padding: f32,
    canvas: vec4<f32>,
};
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;
fn to_doc(p:vec2<f32>)->vec2<f32> {
    let q=(p-view.screen/2.0-view.pan)/view.zoom;
    let c=cos(view.rotation);let s=sin(view.rotation);
    return vec2<f32>((q.x*c+q.y*s)*view.flip,-q.x*s+q.y*c)+view.doc/2.0;
}
fn inside(p:vec2<f32>)->bool {return all(p>=view.canvas.xy) && all(p<view.canvas.xy+view.canvas.zw);}
@vertex fn fullscreen(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let p=array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(p[i],0.0,1.0);
}
@fragment fn background(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
    let d=to_doc(p.xy);
    if view.has_document<0.5 || !inside(d) {return vec4<f32>(48.0/255.0,52.0/255.0,59.0/255.0,1.0);}
    let checker=(u32(floor(d.x/12.0))+u32(floor(d.y/12.0)))%2u;
    let color=select(0.78,0.9,checker==0u);return vec4<f32>(vec3<f32>(color),1.0);
}
@fragment fn grid(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
    let d=to_doc(p.xy);
    if view.grid<0.5 || view.has_document<0.5 || !inside(d) {return vec4<f32>(0.0);}
    let edge=min(fract(d),vec2<f32>(1.0)-fract(d))*view.zoom;
    let a=select(0.0,0.22,min(edge.x,edge.y)<0.5);
    return vec4<f32>(vec3<f32>(0.0),a);
}
@fragment fn outside(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
    if inside(to_doc(p.xy)) {discard;}
    return vec4<f32>(48.0/255.0,52.0/255.0,59.0/255.0,1.0);
}
struct VertexOut {@builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>,@location(1) doc:vec2<f32>};
@vertex fn tile_vertex(@builtin(vertex_index) i:u32,@location(0) rect:vec4<f32>,@location(1) uvrect:vec4<f32>)->VertexOut {
    let corners=array<vec2<f32>,6>(vec2<f32>(0.0,0.0),vec2<f32>(1.0,0.0),vec2<f32>(0.0,1.0),vec2<f32>(0.0,1.0),vec2<f32>(1.0,0.0),vec2<f32>(1.0,1.0));
    let corner=corners[i];let d=rect.xy+corner*rect.zw;
    let q=(d-view.doc/2.0)*vec2<f32>(view.flip,1.0);let c=cos(view.rotation);let s=sin(view.rotation);
    let screen=vec2<f32>(q.x*c-q.y*s,q.x*s+q.y*c)*view.zoom+view.screen/2.0+view.pan;
    var out:VertexOut;out.position=vec4<f32>(screen/view.screen*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),0.0,1.0);
    out.uv=uvrect.xy+corner*uvrect.zw;out.doc=d;return out;
}
@fragment fn tile_fragment(in:VertexOut)->@location(0) vec4<f32> {
    if !inside(in.doc) {discard;}
    return textureSample(atlas,atlas_sampler,in.uv);
}

