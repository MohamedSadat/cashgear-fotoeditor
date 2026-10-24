// sRGB blend functions use straight color. Render targets hold premultiplied
// temporary composites; document textures retain exact straight RGBA8 samples.
struct Params {
    mode: u32,
    operation: u32, // 0: source-over, 1: clipped source-atop, 2: group interpolation
    source_premultiplied: u32,
    mask_enabled: u32,
    opacity: f32,
    fill: f32,
    mask_default: f32,
    adjustment_kind: u32,
    adjustment: vec4<f32>, // HSL hue turns, saturation, lightness, unused
    coverage: vec4<f32>, // enabled, sparse default, unused, unused
    lut: array<vec4<f32>, 256>,
}
@group(0) @binding(0) var backdrop_texture: texture_2d<f32>;
@group(0) @binding(1) var source_texture: texture_2d<f32>;
@group(0) @binding(2) var mask_texture: texture_2d<f32>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var coverage_texture: texture_2d<f32>;

fn adjusted(color: vec3<f32>, coverage: f32) -> vec3<f32> {
    let before = floor(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0 + 0.5) / 255.0;
    var result: vec3<f32>;
    if params.adjustment_kind == 1u {
        let indices = vec3<u32>(floor(before * 255.0 + 0.5));
        result = vec3<f32>(params.lut[indices.r].x, params.lut[indices.g].x, params.lut[indices.b].x);
    } else {
        let high = max(max(before.r, before.g), before.b);
        let low = min(min(before.r, before.g), before.b);
        let delta = high - low;
        var light = (high + low) * 0.5;
        var sat = 0.0;
        var hue = 0.0;
        if delta > 0.0 {
            sat = delta / (1.0 - abs(2.0 * light - 1.0));
            if high == before.r { hue = (before.g - before.b) / delta; }
            else if high == before.g { hue = (before.b - before.r) / delta + 2.0; }
            else { hue = (before.r - before.g) / delta + 4.0; }
        }
        hue = fract(hue / 6.0 + params.adjustment.x) * 6.0;
        sat = clamp(sat * (1.0 + params.adjustment.y), 0.0, 1.0);
        if params.adjustment.z < 0.0 { light *= 1.0 + params.adjustment.z; }
        else { light += (1.0 - light) * params.adjustment.z; }
        let chroma = (1.0 - abs(2.0 * light - 1.0)) * sat;
        let x = chroma * (1.0 - abs(hue - 2.0 * floor(hue / 2.0) - 1.0));
        var rgb: vec3<f32>;
        if hue < 1.0 { rgb = vec3<f32>(chroma, x, 0.0); }
        else if hue < 2.0 { rgb = vec3<f32>(x, chroma, 0.0); }
        else if hue < 3.0 { rgb = vec3<f32>(0.0, chroma, x); }
        else if hue < 4.0 { rgb = vec3<f32>(0.0, x, chroma); }
        else if hue < 5.0 { rgb = vec3<f32>(x, 0.0, chroma); }
        else { rgb = vec3<f32>(chroma, 0.0, x); }
        result = rgb + vec3<f32>(light - chroma * 0.5);
    }
    return floor(clamp(mix(before, result, coverage), vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0 + 0.5) / 255.0;
}

fn luminosity(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.3, 0.59, 0.11)); }
fn saturation(c: vec3<f32>) -> f32 { return max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b); }
fn set_luminosity(color: vec3<f32>, l: f32) -> vec3<f32> {
    var c = color + vec3<f32>(l - luminosity(color));
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    if n < 0.0 { c = vec3<f32>(l) + (c - vec3<f32>(l)) * l / (l - n); }
    if x > 1.0 { c = vec3<f32>(l) + (c - vec3<f32>(l)) * (1.0 - l) / (x - l); }
    return c;
}
fn set_saturation(c: vec3<f32>, s: f32) -> vec3<f32> {
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    if x > n { return (c - vec3<f32>(n)) * s / (x - n); }
    return vec3<f32>(0.0);
}
fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 { return 2.0 * b * s; }
    return 1.0 - 2.0 * (1.0 - b) * (1.0 - s);
}
fn separable(mode: u32, b: f32, s: f32) -> f32 {
    switch mode {
        case 1u: { return b * s; }
        case 2u: { return b + s - b * s; }
        case 3u: { return hard_light(s, b); }
        case 4u: { return min(b, s); }
        case 5u: { return max(b, s); }
        case 6u: {
            if b == 0.0 { return 0.0; }
            if s == 1.0 { return 1.0; }
            return min(1.0, b / (1.0 - s));
        }
        case 7u: {
            if b == 1.0 { return 1.0; }
            if s == 0.0 { return 0.0; }
            return 1.0 - min(1.0, (1.0 - b) / s);
        }
        case 8u: { return hard_light(b, s); }
        case 9u: {
            if s <= 0.5 { return b - (1.0 - 2.0 * s) * b * (1.0 - b); }
            var d = sqrt(b);
            if b <= 0.25 { d = ((16.0 * b - 12.0) * b + 4.0) * b; }
            return b + (2.0 * s - 1.0) * (d - b);
        }
        case 10u: { return abs(b - s); }
        case 11u: { return b + s - 2.0 * b * s; }
        default: { return s; }
    }
}
fn blend_rgb(mode: u32, b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    var c: vec3<f32>;
    switch mode {
        case 12u: { c = set_luminosity(set_saturation(s, saturation(b)), luminosity(b)); }
        case 13u: { c = set_luminosity(set_saturation(b, saturation(s)), luminosity(b)); }
        case 14u: { c = set_luminosity(s, luminosity(b)); }
        case 15u: { c = set_luminosity(b, luminosity(s)); }
        default: { c = vec3<f32>(separable(mode, b.r, s.r), separable(mode, b.g, s.g), separable(mode, b.b, s.b)); }
    }
    return clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
}
@vertex
fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let vertices = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(vertices[i], 0.0, 1.0);
}
@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let xy = vec2<i32>(position.xy);
    let backdrop = textureLoad(backdrop_texture, xy, 0);
    var source = textureLoad(source_texture, xy, 0);
    var mask = params.mask_default;
    if params.mask_enabled != 0u { mask = textureLoad(mask_texture, xy, 0).r; }
    let factor = clamp(params.opacity * params.fill * mask, 0.0, 1.0);
    if params.operation == 3u {
        if backdrop.a == 0.0 { return backdrop; }
        let b = floor(clamp(backdrop.rgb / backdrop.a, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0 + 0.5) / 255.0;
        let rgb = blend_rgb(params.mode, b, adjusted(b, 1.0));
        return vec4<f32>(mix(backdrop.rgb, rgb * backdrop.a, factor), backdrop.a);
    }
    if params.adjustment_kind != 0u && source.a > 0.0 {
        var coverage = params.coverage.y;
        if params.coverage.x > 0.5 { coverage = textureLoad(coverage_texture, xy, 0).r; }
        source = vec4<f32>(adjusted(source.rgb, coverage), source.a);
    }
    if params.operation == 2u { return mix(backdrop, source, factor); }
    let a = source.a * factor;
    let clipped = params.operation == 1u;
    if a == 0.0 || (clipped && backdrop.a == 0.0) { return backdrop; }
    var b = vec3<f32>(0.0);
    if backdrop.a > 0.0 { b = backdrop.rgb / backdrop.a; }
    var s = source.rgb;
    if params.source_premultiplied != 0u { s = source.rgb / source.a; }
    let blended = blend_rgb(params.mode, b, s);
    var contribution = backdrop.a * blended;
    var alpha = backdrop.a;
    if !clipped {
        contribution += (1.0 - backdrop.a) * s;
        alpha = a + backdrop.a * (1.0 - a);
    }
    return vec4<f32>((1.0 - a) * backdrop.rgb + a * contribution, alpha);
}
