
struct Globals {
    view_proj: mat4x4<f32>,
    fog_color: vec4<f32>,
    fog: vec4<f32>,
    sky_color: vec4<f32>,
    misc: vec4<f32>,
    inverse_view: mat4x4<f32>,
    sunrise: vec4<f32>,
    sun: vec4<f32>,
    fog_ends: vec4<f32>,
    end_portal_sprites: array<vec4<f32>, 2>,
    time: vec4<f32>,
    crumbling_sprite: vec4<f32>,
    outline_params: vec4<f32>,
    dynamic_lights: array<vec4<f32>, 16>,
    sunrise_fan: array<vec4<f32>, 32>,
}
@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var sprites: texture_2d_array<f32>;
@group(0) @binding(2) var sprite_sampler: sampler;
@group(0) @binding(3) var lightmap: texture_2d<f32>;
@group(0) @binding(4) var lightmap_sampler: sampler;
@group(0) @binding(5) var sprites_1: texture_2d_array<f32>;
@group(0) @binding(6) var sprites_2: texture_2d_array<f32>;
@group(0) @binding(7) var sprites_3: texture_2d_array<f32>;
@group(0) @binding(8) var sprites_4: texture_2d_array<f32>;
@group(0) @binding(9) var sprites_5: texture_2d_array<f32>;
@group(0) @binding(10) var sprites_6: texture_2d_array<f32>;
@group(0) @binding(11) var sprites_7: texture_2d_array<f32>;
override SPRITE_ARRAYS: u32 = 1u;
override SPRITE_SPLIT: u32 = 2048u;
fn sprite(uv: vec2<f32>, layer: u32) -> vec4<f32> {
    if SPRITE_ARRAYS == 1u {
        return textureSample(sprites, sprite_sampler, uv, i32(layer));
    }
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let l = i32(layer % SPRITE_SPLIT);
    switch layer / SPRITE_SPLIT {
        case 0u: { return textureSampleGrad(sprites, sprite_sampler, uv, l, dx, dy); }
        case 1u: { return textureSampleGrad(sprites_1, sprite_sampler, uv, l, dx, dy); }
        case 2u: { return textureSampleGrad(sprites_2, sprite_sampler, uv, l, dx, dy); }
        case 3u: { return textureSampleGrad(sprites_3, sprite_sampler, uv, l, dx, dy); }
        case 4u: { return textureSampleGrad(sprites_4, sprite_sampler, uv, l, dx, dy); }
        case 5u: { return textureSampleGrad(sprites_5, sprite_sampler, uv, l, dx, dy); }
        case 6u: { return textureSampleGrad(sprites_6, sprite_sampler, uv, l, dx, dy); }
        default: { return textureSampleGrad(sprites_7, sprite_sampler, uv, l, dx, dy); }
    }
}

struct TerrainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) color: vec4<f32>,
    @location(3) spherical: f32,
    @location(4) cylindrical: f32,
    @location(5) @interpolate(flat) emission: u32,
}
fn dynamic_block_light(pos: vec3<f32>) -> f32 {
    let count = min(u32(globals.time.z), 16u);
    var level = 0.0;
    for (var i = 0u; i < count; i++) {
        let l = globals.dynamic_lights[i];
        level = max(level, l.w - distance(pos, l.xyz));
    }
    return level * 16.0;
}
fn sample_lightmap(sky: f32, block: f32) -> vec4<f32> {
    let uv = vec2(block, sky) * 16.0;
    return textureSampleLevel(lightmap, lightmap_sampler,
        clamp(uv / 256.0 + 0.5 / 16.0, vec2(0.5 / 16.0), vec2(15.5 / 16.0)), 0.0);
}
fn terrain_uv(packed: vec2<u32>, step: f32) -> vec2<f32> {
    return vec2<f32>(packed) * step;
}
@vertex fn vs_terrain(
    @location(0) position: vec3<f32>, @location(1) uv: vec2<u32>,
    @location(2) layer: u32, @location(3) color: vec4<f32>, @location(4) light: vec2<u32>,
    @location(5) origin: vec4<f32>) -> TerrainOut {
    return terrain_vertex(position, terrain_uv(uv, origin.w), layer, color, light, origin.xyz);
}
@group(1) @binding(0) var<storage, read> block_slots: array<u32>;
@group(1) @binding(1) var<storage, read> slot_origins: array<vec4<f32>>;
@vertex fn vs_terrain_flat(
    @builtin(vertex_index) vertex: u32,
    @location(0) position: vec3<f32>, @location(1) uv: vec2<u32>,
    @location(2) layer: u32, @location(3) color: vec4<f32>, @location(4) light: vec2<u32>
) -> TerrainOut {
    let origin = slot_origins[block_slots[vertex >> 8u]];
    return terrain_vertex(position, terrain_uv(uv, origin.w), layer, color, light, origin.xyz);
}
fn terrain_vertex(position: vec3<f32>, uv: vec2<f32>, layer: u32, color: vec4<f32>,
    light: vec2<u32>, origin: vec3<f32>) -> TerrainOut {
    let pos = position + origin;
    var out: TerrainOut;
    out.clip = globals.view_proj * vec4(pos, 1.0);
    out.uv = uv;
    out.layer = layer;
    let block = max(f32(light.y), dynamic_block_light(pos));
    out.color = vec4(color.rgb, 1.0) * sample_lightmap(f32(light.x) / 16.0, block / 16.0);
    out.emission = 15u - (u32(round(color.a * 255.0)) & 15u);
    out.spherical = length(pos);
    out.cylindrical = max(length(pos.xz), abs(pos.y));
    return out;
}
fn linear_fog(d: f32, start: f32, end: f32) -> f32 {
    if d <= start { return 0.0; }
    if d >= end { return 1.0; }
    return (d - start) / (end - start);
}
fn apply_fog(color: vec4<f32>, spherical: f32, cylindrical: f32) -> vec4<f32> {
    let f = max(linear_fog(spherical, globals.fog.x, globals.fog.y),
                linear_fog(cylindrical, globals.fog.z, globals.fog.w));
    return vec4(mix(color.rgb, globals.fog_color.rgb, f * globals.fog_color.a), color.a);
}
@fragment fn fs_solid(in: TerrainOut) -> @location(0) vec4<f32> {
    let color = sprite(in.uv, in.layer & 0x7fffu) * in.color;
    if color.a < 0.5 && (in.layer & 0x8000u) == 0u { discard; }
    let emissive = select(1.0, 0.7 * f32(4u - min(in.emission, 4u)) / 4.0, in.emission > 0u);
    return apply_fog(vec4(color.rgb, emissive), in.spherical, in.cylindrical);
}
@fragment fn fs_translucent(in: TerrainOut) -> @location(0) vec4<f32> {
    let color = sprite(in.uv, in.layer & 0x7fffu) * in.color;
    if color.a < 0.01 { discard; }
    return apply_fog(color, in.spherical, in.cylindrical);
}

// Sky (SkyRenderer)
struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}
@vertex fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    let p = vec2(f32(i32(i & 1u) * 4 - 1), f32(i32(i >> 1u) * 4 - 1));
    var out: SkyOut;
    out.clip = vec4(p, 0.0, 1.0);
    out.ndc = p;
    return out;
}
fn view_ray(ndc: vec2<f32>) -> vec3<f32> {
    return normalize((globals.inverse_view * vec4(ndc.x * globals.misc.z * globals.misc.w,
        ndc.y * globals.misc.w, -1.0, 0.0)).xyz);
}
const PI: f32 = 3.14159265;
fn disc_distance(d: vec3<f32>, y: f32) -> vec2<f32> {
    if d.y * y <= 0.0 { return vec2(-1.0); }
    let p = d * (y / d.y);
    let h = length(p.xz);
    let phi = atan2(p.z, p.x);
    let sector = PI / 4.0;
    let delta = phi - (floor(phi / sector) * sector + sector * 0.5);
    let edge = 512.0 * cos(sector * 0.5) / cos(delta);
    if h > edge { return vec2(-1.0); }
    let s = h / edge;
    let ay = abs(y);
    let rim = sqrt(512.0 * 512.0 + y * y);
    return vec2(ay + s * (rim - ay), ay + s * (512.0 - ay));
}
fn sky_fog(color: vec3<f32>, dist: vec2<f32>) -> vec3<f32> {
    let end = globals.fog_ends.x;
    let f = max(linear_fog(dist.x, 0.0, end), linear_fog(dist.y, end, end));
    return mix(color, globals.fog_color.rgb, f * globals.fog_color.a);
}
fn sunrise_weight(d: vec3<f32>) -> f32 {
    if globals.sunrise.a <= 0.001 { return 0.0; }
    var best = 0.0;
    var k0 = dot(d, globals.sunrise_fan[16].xyz);
    for (var i = 0; i < 16; i++) {
        let triangle = globals.sunrise_fan[i];
        let k1 = dot(d, globals.sunrise_fan[16 + (i + 1) % 16].xyz);
        if k0 * triangle.w <= 0.0 && k1 * triangle.w >= 0.0 {
            let apex = dot(d, triangle.xyz);
            let s = apex + k1 - k0;
            if abs(s) >= 1e-6 && s * triangle.w > 0.0 && apex * s >= 0.0 && k1 * s >= 0.0
                && k0 * s <= 0.0 {
                best = max(best, apex / s);
            }
        }
        k0 = k1;
    }
    return best;
}
fn sky_along_ray(d: vec3<f32>) -> vec3<f32> {
    var color = globals.fog_color.rgb;
    let dist = disc_distance(d, 16.0);
    if dist.x >= 0.0 {
        color = sky_fog(globals.sky_color.rgb, dist);
    }
    let w = sunrise_weight(d);
    if w > 0.0 {
        color = mix(color, globals.sunrise.rgb, w * globals.sunrise.a);
    }
    return color;
}
@fragment fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    return vec4(sky_along_ray(view_ray(in.ndc)), 1.0);
}
@fragment fn fs_dark_disc(in: SkyOut) -> @location(0) vec4<f32> {
    let d = view_ray(in.ndc);
    let dist = disc_distance(d, -4.0);
    if dist.x < 0.0 { discard; }
    return vec4(sky_fog(vec3(0.0), dist), 1.0);
}

// rendertype_lines
fn outline_clip(vertex: u32, start: vec3<f32>, end: vec3<f32>, width: f32) -> vec4<f32> {
    let position = select(start, end, vertex >= 2u);
    let normal = normalize(end - start);
    let view_shrink = 1.0 - 1.0 / 256.0;
    let line_start = globals.view_proj * vec4((position * globals.outline_params.w) * view_shrink, 1.0);
    let line_end = globals.view_proj * vec4(((position + normal) * globals.outline_params.w) * view_shrink, 1.0);
    let ndc1 = line_start.xyz / line_start.w;
    let ndc2 = line_end.xyz / line_end.w;
    let screen = globals.outline_params.xy;
    let direction = normalize((ndc2.xy - ndc1.xy) * screen);
    var offset = vec2(-direction.y, direction.x) * width / screen;
    if offset.x < 0.0 { offset = -offset; }
    if vertex % 2u != 0u { offset = -offset; }
    return vec4((ndc1 + vec3(offset, 0.0)) * line_start.w, line_start.w);
}
struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) spherical: f32,
    @location(1) cylindrical: f32,
    @location(2) color: vec4<f32>,
}
@vertex fn vs_line(@builtin(vertex_index) vertex: u32,
    @location(0) start: vec3<f32>, @location(1) end: vec3<f32>,
    @location(2) argb: vec4<f32>, @location(3) width: f32) -> LineOut {
    let position = select(start, end, vertex >= 2u);
    var out: LineOut;
    out.clip = outline_clip(vertex, start, end, width);
    out.spherical = length(position);
    out.cylindrical = max(length(position.xz), abs(position.y));
    out.color = argb.zyxw;
    return out;
}
@fragment fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    return apply_fog(in.color, in.spherical, in.cylindrical);
}
struct DebugLineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) spherical: f32,
    @location(1) cylindrical: f32,
    @location(2) color: vec4<f32>,
}
@vertex fn vs_debug_line(@builtin(vertex_index) vertex: u32,
    @location(0) start: vec3<f32>, @location(1) end: vec3<f32>,
    @location(2) argb: vec4<f32>, @location(3) width: f32) -> DebugLineOut {
    let position = select(start, end, vertex >= 2u);
    let view_shrink = 1.0 - 1.0 / 256.0;
    let line_start = globals.view_proj * vec4(position * view_shrink, 1.0);
    let line_end = globals.view_proj * vec4((position + (end - start)) * view_shrink, 1.0);
    let ndc1 = line_start.xyz / line_start.w;
    let ndc2 = line_end.xyz / line_end.w;
    let screen = globals.outline_params.xy;
    let direction = normalize((ndc2.xy - ndc1.xy) * screen);
    var offset = vec2(-direction.y, direction.x) * width / screen;
    if offset.x < 0.0 { offset = -offset; }
    if vertex % 2u != 0u { offset = -offset; }
    var out: DebugLineOut;
    out.clip = vec4((ndc1 + vec3(offset, 0.0)) * line_start.w, line_start.w);
    out.spherical = length(position);
    out.cylindrical = max(length(position.xz), abs(position.y));
    out.color = argb.zyxw;
    return out;
}
@fragment fn fs_debug_line(in: DebugLineOut) -> @location(0) vec4<f32> {
    return apply_fog(in.color, in.spherical, in.cylindrical);
}
