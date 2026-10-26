struct Params {
    row0:vec4<f32>,row1:vec4<f32>,row2:vec4<f32>,
    origin:vec4<f32>,geometry:vec4<f32>, // document pixel center, x/y deltas, source size
    cache:vec4<f32>, // cache size, interpolation, selected-pixel mode
    canvas:vec4<f32>,flags:vec4<f32>, // visible source rectangle; mask enabled/default
    domain:vec4<f32>, // cache origin and document-to-cache scale
}
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var auxiliary:texture_2d<f32>;
@group(0) @binding(2) var<uniform> params:Params;
@vertex fn vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{
    let p=array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));return vec4<f32>(p[i],0.0,1.0);
}
fn inside(p:vec2<f32>,r:vec4<f32>)->bool{return all(p>=r.xy)&&all(p<r.xy+r.zw);}
fn at(p:vec2<i32>)->vec4<f32>{if any(p<vec2<i32>(0))||any(p>=vec2<i32>(params.cache.xy)){return vec4<f32>(0.0);}return textureLoad(source,p,0);}
fn aux(p:vec2<i32>)->vec2<f32>{if any(p<vec2<i32>(0))||any(p>=vec2<i32>(params.cache.xy)){return vec2<f32>(0.0,params.flags.y);}return textureLoad(auxiliary,p,0).rg;}
fn coverage(p:vec2<i32>)->f32{let d=(vec2<f32>(p)+vec2<f32>(0.5))/params.domain.zw+params.domain.xy;if !inside(d,params.canvas){return 0.0;}return aux(p).r;}
fn weight(raw:f32)->f32{
    let x=abs(raw);
    if params.cache.z<1.5{return max(0.0,1.0-x);}
    if params.cache.z<2.5{if x<1.0{return 1.5*x*x*x-2.5*x*x+1.0;}if x<2.0{return -0.5*x*x*x+2.5*x*x-4.0*x+2.0;}return 0.0;}
    if x<0.00001{return 1.0;}if x>=3.0{return 0.0;}let p=3.14159265359*x;return sin(p)/p*sin(p/3.0)/(p/3.0);
}
fn quantize(p:vec4<f32>)->vec4<f32>{let a=floor(clamp(p.a,0.0,1.0)*255.0+0.5)/255.0;if a<=0.0{return vec4<f32>(0.0);}let rgb=floor(clamp(p.rgb/max(p.a,0.0000001),vec3<f32>(0.0),vec3<f32>(1.0))*255.0+0.5)/255.0;return vec4<f32>(rgb*a,a);}
struct Sample {color:vec4<f32>,mask:f32}
fn sample_transformed(edge:vec2<f32>,scale:vec2<f32>)->Sample{
    let center=edge-vec2<f32>(0.5);var result=vec4<f32>(0.0);var mask=0.0;var total=0.0;
    let exact=params.cache.z<0.5||(all(scale<=vec2<f32>(1.00001))&&all(abs(center-round(center))<vec2<f32>(0.00001)));
    if exact {let p=vec2<i32>(floor(edge));var v=at(p);if params.cache.w>0.5{v.a=floor(v.a*coverage(p)*255.0+0.5)/255.0;}return Sample(vec4<f32>(v.rgb*v.a,v.a),aux(p).g);}
    let support=select(select(3.0,2.0,params.cache.z<2.5),1.0,params.cache.z<1.5);
    let low=vec2<i32>(ceil(center-support*scale));let high=vec2<i32>(floor(center+support*scale));
    for(var y=low.y;y<=high.y;y++){let wy=weight((f32(y)-center.y)/scale.y);for(var x=low.x;x<=high.x;x++){
        let w=weight((f32(x)-center.x)/scale.x)*wy;let p=vec2<i32>(x,y);var v=at(p);if params.cache.w>0.5{v.a=floor(v.a*coverage(p)*255.0+0.5)/255.0;}
        result+=vec4<f32>(v.rgb*v.a,v.a)*w;mask+=aux(p).g*w;total+=w;
    }}
    if abs(total)>0.0000001{return Sample(result/total,mask/total);}return Sample(vec4<f32>(0.0),params.flags.y);
}
@fragment fn fragment(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32>{
    let i=floor(pos.xy);let raw=params.origin.xy+i.x*params.origin.zw+i.y*params.geometry.xy;
    if !inside(raw,params.canvas){return vec4<f32>(0.0);}
    let doc=floor(raw)+vec2<f32>(0.5);let base_at=vec2<i32>(floor((doc-params.domain.xy)*params.domain.zw));
    var base=vec4<f32>(0.0);
    if params.cache.w>0.5{let p=at(base_at);let a=floor(p.a*(1.0-coverage(base_at))*255.0+0.5)/255.0;base=vec4<f32>(p.rgb*a,a);}
    let homogeneous=vec3<f32>(doc,1.0);let denominator=dot(params.row2.xyz,homogeneous);
    var transformed=vec4<f32>(0.0);
    if abs(denominator)>0.00000001{
        let p=vec2<f32>(dot(params.row0.xyz,homogeneous),dot(params.row1.xyz,homogeneous))/denominator;
        let margin=vec2<f32>(12.0)/params.domain.zw;
        if all(p>=-margin)&&all(p<params.geometry.zw+margin){
            let dx=vec2<f32>(params.row0.x-p.x*params.row2.x,params.row1.x-p.y*params.row2.x)/denominator;
            let dy=vec2<f32>(params.row0.y-p.x*params.row2.y,params.row1.y-p.y*params.row2.y)/denominator;
            let scale=clamp(vec2<f32>(length(vec2<f32>(dx.x,dy.x)),length(vec2<f32>(dx.y,dy.y)))*params.domain.zw,vec2<f32>(1.0),vec2<f32>(4.0));
            let value=sample_transformed((p-params.domain.xy)*params.domain.zw,scale);transformed=quantize(value.color);
            if params.cache.w<0.5&&params.flags.x>0.5{transformed*=floor(clamp(value.mask,0.0,1.0)*255.0+0.5)/255.0;}
        }
    }
    var result=transformed+base*(1.0-transformed.a);if params.cache.w>0.5{result=quantize(result);}
    if result.a<=0.0{return vec4<f32>(0.0);}return vec4<f32>(result.rgb/result.a,result.a);
}
