
struct Params {
    stamp: u32,
    big: f32,
}
struct Globals {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<storage, read> tests: array<vec4<u32>>;
@group(0) @binding(1) var<storage, read_write> results: array<u32>;
@group(0) @binding(2) var<storage, read_write> phase2: array<u32>;
@group(0) @binding(3) var<storage, read> origins: array<vec4<f32>>;
@group(0) @binding(4) var<uniform> globals: Globals;
@group(0) @binding(5) var<uniform> params: Params;

const CORNERS = array<u32, 36>(
    0u, 6u, 2u, 0u, 4u, 6u, 1u, 3u, 7u, 1u, 7u, 5u, 0u, 1u, 5u, 0u, 5u, 4u,
    2u, 7u, 3u, 2u, 6u, 7u, 0u, 3u, 1u, 0u, 2u, 3u, 4u, 5u, 7u, 4u, 7u, 6u,
);

struct Box {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) test: u32,
}

fn corner(origin: vec3<f32>, c: u32) -> vec3<f32> {
    return origin + vec3(
        select(-1.0, 17.0, (c & 1u) != 0u),
        select(-1.0, 17.0, (c & 2u) != 0u),
        select(-1.0, 17.0, (c & 4u) != 0u),
    );
}

@vertex
fn vs_box(@builtin(vertex_index) v: u32, @builtin(instance_index) t: u32) -> Box {
    let origin = origins[tests[t].x].xyz;
    var out: Box;
    out.test = t;
    var lo = vec2(1e30);
    var hi = vec2(-1e30);
    var behind = false;
    for (var c = 0u; c < 8u; c++) {
        let p = globals.view_proj * vec4(corner(origin, c), 1.0);
        behind = behind || p.w <= 0.0;
        let q = p.xy / max(p.w, 1e-6);
        lo = min(lo, q);
        hi = max(hi, q);
    }
    let size = max(min(hi, vec2(1.0)) - max(lo, vec2(-1.0)), vec2(0.0));
    let near = all(origin < vec3(2.0)) && all(origin > vec3(-18.0));
    if near || (!behind && size.x * size.y > params.big * 4.0) {
        var p = vec2(-1.0, 1.0);
        if v == 1u {
            p.y -= 0.05;
        } else if v == 2u {
            p.x += 0.05;
        }
        out.position = select(vec4(0.0, 0.0, 0.0, 1.0), vec4(p, 1.0, 1.0), v < 3u);
        return out;
    }
    out.position = globals.view_proj * vec4(corner(origin, CORNERS[v]), 1.0);
    return out;
}

fn see(t: u32) {
    if results[t] != params.stamp {
        results[t] = params.stamp;
        let test = tests[t];
        for (var k = 0u; k < test.z; k++) {
            phase2[(test.y + k) * 5u + 1u] = 1u;
        }
    }
}

@group(1) @binding(0) var depth: texture_depth_2d;

@fragment
fn fs_box(in: Box) {
    if in.position.z >= textureLoad(depth, vec2<u32>(in.position.xy), 0) {
        see(in.test);
    }
}
