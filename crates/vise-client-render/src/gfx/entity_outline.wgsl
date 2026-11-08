@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct PostOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs_quad(@builtin(vertex_index) i: u32) -> PostOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: PostOut;
    out.clip = vec4(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2(p.x, 1.0 - p.y);
    return out;
}

fn tap(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(source, source_sampler, uv, 0.0);
}

@fragment fn fs_sobel(in: PostOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    let center = tap(in.uv);
    let left = tap(in.uv - vec2(texel.x, 0.0));
    let right = tap(in.uv + vec2(texel.x, 0.0));
    let up = tap(in.uv - vec2(0.0, texel.y));
    let down = tap(in.uv + vec2(0.0, texel.y));
    let total = clamp(abs(center.a - left.a) + abs(center.a - right.a)
        + abs(center.a - up.a) + abs(center.a - down.a), 0.0, 1.0);
    let color = center.rgb * center.a + left.rgb * left.a + right.rgb * right.a
        + up.rgb * up.a + down.rgb * down.a;
    return vec4(color * 0.2, total);
}

fn blur(uv: vec2<f32>, step: vec2<f32>) -> vec4<f32> {
    let blurred = tap(uv + step * -1.5) + tap(uv + step * 0.5) + tap(uv + step * 2.0) / 2.0;
    return vec4(blurred.rgb / 2.5, blurred.a);
}

@fragment fn fs_blur_x(in: PostOut) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2(1.0 / f32(textureDimensions(source).x), 0.0));
}

@fragment fn fs_blur_y(in: PostOut) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2(0.0, -1.0 / f32(textureDimensions(source).y)));
}

@fragment fn fs_blit(in: PostOut) -> @location(0) vec4<f32> {
    return tap(in.uv);
}
