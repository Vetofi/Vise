@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var world: texture_2d<f32>;

struct Params {
    paper: f32,
    peak: f32,
    world: f32,
    debug: f32,
    pq: f32,
    black: f32,
    ui: f32,
    share: vec4<f32>,
    start: vec4<f32>,
    full: vec4<f32>,
};
@group(0) @binding(2) var<uniform> params: Params;

fn shows_world(c: vec3<f32>, w: vec3<f32>) -> bool {
    if all(c == w) {
        return true;
    }
    let f = (c.r + c.g + c.b) / max(w.r + w.g + w.b, 1.0 / 255.0);
    return f <= 1.01 && f > 0.5 && all(abs(c - f * w) <= 0.03 * w + 1.0 / 255.0);
}

const BT709_TO_BT2020 = mat3x3<f32>(
    vec3(0.6274, 0.0691, 0.0164),
    vec3(0.3293, 0.9195, 0.0880),
    vec3(0.0433, 0.0114, 0.8956),
);

fn pq(nits: vec3<f32>) -> vec3<f32> {
    let y = pow(clamp(nits / 10000.0, vec3(0.0), vec3(1.0)), vec3(0.1593017578125));
    return pow((0.8359375 + 18.8515625 * y) / (1.0 + 18.6875 * y), vec3(78.84375));
}

fn pq_nits(e: vec3<f32>) -> vec3<f32> {
    let p = pow(clamp(e, vec3(0.0), vec3(1.0)), vec3(1.0 / 78.84375));
    let y = max(p - 0.8359375, vec3(0.0)) / (18.8515625 - 18.6875 * p);
    return 10000.0 * pow(y, vec3(1.0 / 0.1593017578125));
}

fn lift_black(e: vec3<f32>) -> vec3<f32> {
    let top = pq(vec3(params.peak * params.paper * 80.0)).x;
    let n = min(e / top, vec3(1.0));
    return e + pq(vec3(params.black)).x * pow(vec3(1.0) - n, vec3(4.0));
}

@vertex fn vs_hdr(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment fn fs_clear_alpha() -> @location(0) vec4<f32> {
    return vec4(0.0);
}

@fragment fn fs_hdr(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let at = vec2<i32>(pos.xy);
    let f = textureLoad(frame, at, 0);
    let c = f.rgb;
    let covered = f.a > 0.0;
    let ui = select(f.a, 0.0, f.a < 1.5 / 255.0);
    var light = pow(max(c, vec3(0.0)), vec3(2.2));
    let w = textureLoad(world, at, 0);
    var lifted = false;
    if params.world > 0.5 && params.peak > 1.0 && w.a < 0.75 && !covered
        && shows_world(c, w.rgb) {
        let wl = pow(max(w.rgb, vec3(0.0)), vec3(2.2));
        let y = 0.5 * (dot(wl, vec3(0.2126, 0.7152, 0.0722)) + max(wl.r, max(wl.g, wl.b)));
        let tier = u32(clamp(round(4.0 - 4.0 * w.a / 0.7), 1.0, 4.0)) - 1u;
        let lift = smoothstep(params.start[tier], params.full[tier], y);
        if lift > 0.0 {
            let top = max(light.r, max(light.g, light.b));
            let gain = min(1.0 + (params.peak - 1.0) * params.share[tier] * lift, params.peak / max(top, 1e-4));
            light *= max(gain, 1.0);
            lifted = true;
        }
    }
    if params.debug > 0.5 && params.world > 0.5 {
        if lifted {
            light *= vec3(1.0, 0.6, 1.0);
        } else {
            light = vec3(dot(light, vec3(0.2126, 0.7152, 0.0722)));
        }
    }
    light *= mix(1.0, params.ui, ui);
    let scrgb = light * params.paper;
    if params.pq > 0.5 {
        let e = pq(BT709_TO_BT2020 * scrgb * 80.0);
        return vec4(select(e, lift_black(e), params.black > 0.0), 1.0);
    }
    if params.black > 0.0 {
        return vec4(pq_nits(lift_black(pq(scrgb * 80.0))) / 80.0, 1.0);
    }
    return vec4(scrgb, 1.0);
}
