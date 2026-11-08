struct Stars {
    clip_from_sky: mat4x4<f32>,
    color: vec4<f32>,
}
@group(0) @binding(0) var<uniform> stars: Stars;

@vertex fn vs_stars(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return stars.clip_from_sky * vec4(position, 1.0);
}

@fragment fn fs_stars() -> @location(0) vec4<f32> {
    return stars.color;
}
