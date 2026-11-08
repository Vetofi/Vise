override contrast: f32 = 1.2;

fn material(input: MaterialInput) -> Surface {
    let light = pow(input.lightmap, vec3<f32>(contrast));
    return Surface(input.albedo * input.tint * vec4<f32>(light, 1.0));
}
