@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var nearest_sampler: sampler;
@group(0) @binding(4) var linear_sampler: sampler;
override CLAMP: bool = false;
struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) edges: vec4<f32>,
}
@vertex fn vs_screen(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>,
    @location(2) rect: vec4<f32>, @location(3) color: vec4<f32>) -> Out {
    var out: Out;
    out.clip = vec4(pos, 0.0, 1.0);
    out.uv = uv;
    out.rect = rect;
    out.color = color;
    let half = 0.5 / vec2<f32>(textureDimensions(atlas));
    out.edges = vec4(rect.xy + half, rect.zw - half);
    return out;
}
fn atlas_uv(in: Out) -> vec2<f32> {
    var f = in.uv;
    if !CLAMP {
        f = fract(f);
    }
    let uv = in.rect.xy + f * (in.rect.zw - in.rect.xy);
    if CLAMP {
        return clamp(uv, in.edges.xy, in.edges.zw);
    }
    return uv;
}
fn screen(c: vec4<f32>) -> vec4<f32> {
    if c.a <= 0.0 { discard; }
    return c;
}
@fragment fn fs_screen(in: Out) -> @location(0) vec4<f32> {
    return screen(textureSampleLevel(atlas, nearest_sampler, atlas_uv(in), 0.0) * in.color);
}
@fragment fn fs_screen_blur(in: Out) -> @location(0) vec4<f32> {
    return screen(textureSampleLevel(atlas, linear_sampler, atlas_uv(in), 0.0) * in.color);
}
@fragment fn fs_vignette(in: Out) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(atlas, nearest_sampler, atlas_uv(in), 0.0) * in.color;
    return vec4(c.rgb, 1.0);
}
@fragment fn fs_vignette_blur(in: Out) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(atlas, linear_sampler, atlas_uv(in), 0.0) * in.color;
    return vec4(c.rgb, 1.0);
}
