struct Params {
    kind: u32, radius: i32, amount: f32, threshold: f32,
    alpha_lock: u32, unused0: u32, unused1: u32, unused2: u32,
    origin: vec4<f32>, // document origin xy, document delta per screen x
    geometry: vec4<f32>, // document delta per screen y, document size
    weights: array<vec4<f32>,129>,
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var horizontal: texture_2d<f32>;
@group(0) @binding(2) var coverage: texture_2d<f32>;
@group(0) @binding(3) var<uniform> params: Params;
@vertex fn vertex(@builtin(vertex_index) i:u32) -> @builtin(position) vec4<f32> {
    let p=array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(p[i],0.0,1.0);
}
fn input(p:vec2<i32>)->vec4<f32> {
    let s=textureLoad(source,clamp(p,vec2<i32>(0),vec2<i32>(383)),0);
    return vec4<f32>(s.rgb*s.a,s.a);
}
@fragment fn first(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32> {
    let p=vec2<i32>(pos.xy);let r=params.radius;
    var result=input(p)*params.weights[u32(r)].x;
    for(var k=1;k<=r;k++) {result+=(input(p-vec2<i32>(k,0))+input(p+vec2<i32>(k,0)))*params.weights[u32(r+k)].x;}
    return result;
}
@fragment fn finish(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32> {
    let at=vec2<i32>(pos.xy);let p=at+vec2<i32>(64);let r=params.radius;
    let doc=params.origin.xy+vec2<f32>(at).x*params.origin.zw+vec2<f32>(at).y*params.geometry.xy;
    if any(doc<vec2<f32>(0.0)) || any(doc>=params.geometry.zw) {return vec4<f32>(0.0);}
    let original=textureLoad(source,p,0);let before=vec4<f32>(original.rgb*original.a,original.a);
    var result=textureLoad(horizontal,p,0)*params.weights[u32(r)].x;
    if params.kind==3u {
        // Exact component median of premultiplied byte products. A fixed 16
        // bisections finds the integer order statistic in [0,65025].
        // Integer-valued floats are exact throughout this range and avoid
        // integer-vector mix, which is unavailable in GLSL ES 3.00.
        var low=vec4<f32>(0.0);var high=vec4<f32>(65025.0);
        let middle=(2*r+1)*(2*r+1)/2;
        for(var bit=0;bit<16;bit++) {
            let pivot=floor((low+high)*0.5);var count=vec4<f32>(0.0);
            for(var y=-r;y<=r;y++) {for(var x=-r;x<=r;x++) {
                let v=floor(input(p+vec2<i32>(x,y))*65025.0+0.5);
                count+=select(vec4<f32>(0.0),vec4<f32>(1.0),v<=pivot);
            }}
            let lower=count>vec4<f32>(f32(middle));
            high=select(high,pivot,lower);low=select(pivot+vec4<f32>(1.0),low,lower);
        }
        result=vec4<f32>(low)/65025.0;
    } else {
        for(var k=1;k<=r;k++) {result+=(textureLoad(horizontal,p-vec2<i32>(0,k),0)+textureLoad(horizontal,p+vec2<i32>(0,k),0))*params.weights[u32(r+k)].x;}
    }
    if params.kind==2u {
        var blurred=vec3<f32>(0.0);if result.a>0.0 {blurred=result.rgb/result.a;}
        let delta=original.rgb-blurred;
        let rgb=clamp(original.rgb+params.amount*delta,vec3<f32>(0.0),vec3<f32>(1.0));
        result=vec4<f32>(select(original.rgb,rgb,abs(delta)>=vec3<f32>(params.threshold))*original.a,original.a);
    }
    if params.alpha_lock!=0u {
        var rgb=vec3<f32>(0.0);if result.a>0.0 {rgb=result.rgb/result.a;}
        result=vec4<f32>(rgb*original.a,original.a);
    }
    result=mix(before,result,textureLoad(coverage,at,0).r);
    let alpha=floor(clamp(result.a,0.0,1.0)*255.0+0.5)/255.0;
    if alpha==0.0 {return vec4<f32>(original.rgb,0.0);}
    let rgb=floor(clamp(result.rgb/result.a,vec3<f32>(0.0),vec3<f32>(1.0))*255.0+0.5)/255.0;
    return vec4<f32>(rgb,alpha);
}
