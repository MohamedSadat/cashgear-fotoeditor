struct Params {
    kind: u32, radius: i32, amount: f32, threshold: f32,
    alpha_lock: u32, seed: u32, monochrome: u32, distribution: u32,
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
fn docpoint(p:vec2<f32>)->vec2<f32> {return params.origin.xy+p.x*params.origin.zw+p.y*params.geometry.xy;}
fn screen_delta(p:vec2<f32>)->vec2<i32> {
    let dx=params.origin.zw;let dy=params.geometry.xy;let det=dx.x*dy.y-dx.y*dy.x;
    let v=vec2<f32>(p.x*dy.y-p.y*dy.x,p.y*dx.x-p.x*dx.y)/det;
    return vec2<i32>(sign(v)*floor(abs(v)+0.5));
}
fn straight(p:vec4<f32>)->vec3<f32> {if p.a>0.0 {return p.rgb/p.a;}return vec3<f32>(0.0);}
fn hash(value:u32)->u32 {
    var v=value;v=v^(v>>16u);v=v*0x7feb352du;v=v^(v>>15u);v=v*0x846ca68bu;return v^(v>>16u);
}
fn random_uniform(key:u32)->f32 {return (f32(hash(key)>>8u)+0.5)/16777216.0;}
fn noise(point:vec2<u32>,channel:u32)->f32 {
    let key=(point.x*0x9e3779b9u)^(point.y*0x85ebca6bu)^params.seed^(channel*0xc2b2ae35u);
    let u=random_uniform(key);
    if params.distribution==0u {return 2.0*u-1.0;}
    return sqrt(-2.0*log(u))*cos(6.28318530718*random_uniform(key^0xa511e9b3u))/3.0;
}
@fragment fn first(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32> {
    let p=vec2<i32>(pos.xy);let r=params.radius;
    if params.kind==8u {
        let doc=docpoint(vec2<f32>(p)-vec2<f32>(64.0));
        let start=f32(u32(max(0.0,floor(doc.x)))/u32(params.amount))*params.amount;
        let count=max(1.0,min(params.amount,params.geometry.z-start));
        var sum=vec4<f32>(0.0);
        for(var k=0;k<64;k++) {if f32(k)>=count {break;}
            sum+=input(p+screen_delta(vec2<f32>(start+f32(k)+0.5-doc.x,0.0)));
        }
        return sum/count;
    }
    if params.kind==4u || params.kind==6u || params.kind==7u {return input(p);}
    var result=input(p)*params.weights[u32(r)].x;
    for(var k=1;k<=r;k++) {result+=(input(p-vec2<i32>(k,0))+input(p+vec2<i32>(k,0)))*params.weights[u32(r+k)].x;}
    return result;
}
@fragment fn finish(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32> {
    let at=vec2<i32>(pos.xy);let p=at+vec2<i32>(64);let r=params.radius;
    let doc=docpoint(vec2<f32>(at));
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
    } else if params.kind==4u {
        result=vec4<f32>(0.0);
        for(var k=0;k<=2*r;k++) {result+=input(p+vec2<i32>(params.weights[u32(k)].yz));}
        result/=f32(2*r+1);
    } else if params.kind==6u {
        let point=vec2<u32>(floor(doc));var n=vec3<f32>(noise(point,0u));
        if params.monochrome==0u {n=vec3<f32>(n.r,noise(point,1u),noise(point,2u));}
        result=vec4<f32>(clamp(original.rgb+params.amount*n,vec3<f32>(0.0),vec3<f32>(1.0))*original.a,original.a);
    } else if params.kind==7u {
        result=before;
        if r>0 && params.amount>0.0 {
            let delta=vec2<i32>(params.weights[u32(2*r)].yz);
            let rgb=vec3<f32>(128.0/255.0)+params.amount*(straight(input(p+delta))-straight(input(p-delta)))*0.5;
            result=vec4<f32>(clamp(rgb,vec3<f32>(0.0),vec3<f32>(1.0))*original.a,original.a);
        }
    } else if params.kind==8u {
        let start=f32(u32(max(0.0,floor(doc.y)))/u32(params.amount))*params.amount;
        let count=max(1.0,min(params.amount,params.geometry.w-start));
        result=vec4<f32>(0.0);
        for(var k=0;k<64;k++) {if f32(k)>=count {break;}
            let offset=screen_delta(vec2<f32>(0.0,start+f32(k)+0.5-doc.y));
            result+=textureLoad(horizontal,clamp(p+offset,vec2<i32>(0),vec2<i32>(383)),0);
        }
        result/=count;
    } else {
        for(var k=1;k<=r;k++) {result+=(textureLoad(horizontal,p-vec2<i32>(0,k),0)+textureLoad(horizontal,p+vec2<i32>(0,k),0))*params.weights[u32(r+k)].x;}
    }
    if params.kind==5u {
        if r==0 {result=before;}
        else {result=vec4<f32>(clamp(vec3<f32>(128.0/255.0)+original.rgb-straight(result),vec3<f32>(0.0),vec3<f32>(1.0))*original.a,original.a);}
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
