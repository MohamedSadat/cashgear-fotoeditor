struct Settings { mask:vec2<f32>, screen:vec2<f32>, quick:f32, time:f32, padding:vec2<f32> };
@group(0) @binding(0) var mask:texture_2d<f32>;
@group(0) @binding(1) var<uniform> settings:Settings;
@vertex fn vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let p=array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(p[i],0.0,1.0);
}
fn selected(p:vec2<i32>)->bool {
    if any(p<vec2<i32>(0)) || any(p>=vec2<i32>(settings.mask)) {return false;}
    let m=textureLoad(mask,p,0);return m.r>=0.5 && m.g>=0.5;
}
@fragment fn fragment(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
    let q=clamp(vec2<i32>(p.xy/settings.screen*settings.mask),vec2<i32>(0),vec2<i32>(settings.mask)-vec2<i32>(1));
    let m=textureLoad(mask,q,0);
    if m.g<0.5 {discard;}
    if settings.quick>0.5 {let a=(1.0-m.r)*0.45;return vec4<f32>(a,0.0,0.0,a);}
    if !selected(q) {discard;}
    if selected(q+vec2<i32>(-1,0)) && selected(q+vec2<i32>(1,0)) && selected(q+vec2<i32>(0,-1)) && selected(q+vec2<i32>(0,1)) {discard;}
    let stripe=u32(floor((p.x+p.y+settings.time*16.0)/4.0))%2u;
    return vec4<f32>(vec3<f32>(f32(stripe)),1.0);
}
