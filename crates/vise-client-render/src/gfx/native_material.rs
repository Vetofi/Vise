
use vise_shader_api::{check::CheckedStage, stages::GROUP};

use super::native_post::{NativeFrame, present_constants};
use super::wgsl_prefix::{TERRAIN, prefixed, section};

const PREFIX: &str = "vise_terrain_";

const ENTRIES: &str = "
struct ViseMaterialOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) tint: vec4<f32>,
    @location(3) light: vec2<f32>,
    @location(4) lightmap: vec3<f32>,
    @location(5) position: vec3<f32>,
    @location(6) spherical: f32,
    @location(7) cylindrical: f32,
}
fn vise_material_vertex(position: vec3<f32>, uv: vec2<f32>, layer: u32, color: vec4<f32>,
    light: vec2<u32>, origin: vec3<f32>) -> ViseMaterialOut {
    let pos = position + origin;
    var out: ViseMaterialOut;
    out.clip = vise_terrain_globals.view_proj * vec4(pos, 1.0);
    out.uv = uv;
    out.layer = layer;
    out.tint = vec4(color.rgb, 1.0);
    let block = max(f32(light.y), vise_terrain_dynamic_block_light(pos));
    let sky = f32(light.x);
    // 16 per level, 15 levels.
    out.light = min(vec2(block, sky) / 240.0, vec2(1.0));
    out.lightmap = vise_terrain_sample_lightmap(sky / 16.0, block / 16.0).rgb;
    out.position = pos;
    out.spherical = length(pos);
    out.cylindrical = max(length(pos.xz), abs(pos.y));
    return out;
}
@vertex fn vise_material_vs(
    @location(0) position: vec3<f32>, @location(1) uv: vec2<u32>,
    @location(2) layer: u32, @location(3) color: vec4<f32>, @location(4) light: vec2<u32>,
    @location(5) origin: vec4<f32>) -> ViseMaterialOut {
    return vise_material_vertex(position, vise_terrain_terrain_uv(uv, origin.w), layer,
        color, light, origin.xyz);
}
@vertex fn vise_material_vs_flat(
    @builtin(vertex_index) vertex: u32,
    @location(0) position: vec3<f32>, @location(1) uv: vec2<u32>,
    @location(2) layer: u32, @location(3) color: vec4<f32>, @location(4) light: vec2<u32>
) -> ViseMaterialOut {
    let origin = vise_terrain_slot_origins[vise_terrain_block_slots[vertex >> 8u]];
    return vise_material_vertex(position, vise_terrain_terrain_uv(uv, origin.w), layer,
        color, light, origin.xyz);
}
fn vise_material_color(in: ViseMaterialOut, translucent: bool) -> vec4<f32> {
    let albedo = vise_terrain_sprite(in.uv, in.layer & 0x7fffu);
    return material(MaterialInput(albedo, in.tint, in.light, in.lightmap, in.position, in.uv,
        translucent)).color;
}
@fragment fn vise_material_fs_solid(in: ViseMaterialOut) -> @location(0) vec4<f32> {
    let color = vise_material_color(in, false);
    // Bit 15 of the layer marks opaque leaves (forceOpaque): no alpha cutout.
    if color.a < 0.5 && (in.layer & 0x8000u) == 0u { discard; }
    return vise_terrain_apply_fog(vec4(color.rgb, 1.0), in.spherical, in.cylindrical);
}
@fragment fn vise_material_fs_translucent(in: ViseMaterialOut) -> @location(0) vec4<f32> {
    let color = vise_material_color(in, true);
    if color.a < 0.01 { discard; }
    return vise_terrain_apply_fog(color, in.spherical, in.cylindrical);
}
";

pub struct TerrainShape<'a> {
    pub globals: &'a wgpu::BindGroupLayout,
    pub flat: Option<&'a wgpu::BindGroupLayout>,
    pub sprites: [f64; 2],
    pub format: wgpu::TextureFormat,
}

pub struct NativeMaterial {
    pub solid: wgpu::RenderPipeline,
    pub translucent: wgpu::RenderPipeline,
    pub flat: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
}

fn vanilla_terrain() -> String {
    prefixed(
        section(TERRAIN, "struct Globals {", "// Sky (SkyRenderer)"),
        PREFIX,
    )
}

fn shader_source(source: &str) -> String {
    format!(
        "{source}\n// Vise's terrain (terrain.wgsl)\n{}{ENTRIES}",
        vanilla_terrain()
    )
}

fn module(stage: &CheckedStage) -> Result<naga::Module, String> {
    let text = shader_source(&stage.source);
    naga::front::wgsl::parse_str(&text)
        .map_err(|e| format!("{}: {}", stage.file, e.emit_to_string(&text)))
}

impl NativeMaterial {
    pub fn new(
        device: &wgpu::Device,
        shape: &TerrainShape<'_>,
        frame: &NativeFrame,
        stage: &CheckedStage,
        constants: &[(String, f64)],
    ) -> Result<Self, String> {
        let module = module(stage)?;
        let mut constants = present_constants(&module, constants);
        let sprite_names = [
            "vise_terrain_SPRITE_ARRAYS",
            "vise_terrain_SPRITE_SPLIT",
        ];
        constants.extend(sprite_names.into_iter().zip(shape.sprites));
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&stage.label),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
        });
        let layout = |flat: Option<&wgpu::BindGroupLayout>| {
            let mut groups: [Option<&wgpu::BindGroupLayout>; GROUP as usize + 1] =
                Default::default();
            groups[0] = Some(shape.globals);
            groups[1] = flat;
            groups[GROUP as usize] = Some(&frame.layout);
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Native shader pack terrain"),
                bind_group_layouts: &groups,
                ..Default::default()
            })
        };
        let instance_attributes = wgpu::vertex_attr_array![5=>Float32x4];
        let vertices = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<super::TerrainVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &super::TERRAIN_ATTRIBUTES,
        };
        let instances = wgpu::VertexBufferLayout {
            array_stride: 16,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance_attributes,
        };
        let pipeline = |layout: &wgpu::PipelineLayout,
                        vertex: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
                        fragment: &str,
                        blend: Option<wgpu::BlendState>| {
            let options = wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&stage.label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vertex),
                    compilation_options: options.clone(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    compilation_options: options,
                    targets: &[Some(wgpu::ColorTargetState {
                        format: shape.format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let instanced = layout(None);
        let buffers = [Some(vertices.clone()), Some(instances)];
        let solid = pipeline(
            &instanced,
            "vise_material_vs",
            &buffers,
            "vise_material_fs_solid",
            None,
        );
        let translucent = pipeline(
            &instanced,
            "vise_material_vs",
            &buffers,
            "vise_material_fs_translucent",
            blend,
        );
        let flat = shape.flat.map(|group| {
            let layout = layout(Some(group));
            let buffers = [Some(vertices.clone())];
            let vs = "vise_material_vs_flat";
            (
                pipeline(&layout, vs, &buffers, "vise_material_fs_solid", None),
                pipeline(
                    &layout,
                    vs,
                    &buffers,
                    "vise_material_fs_translucent",
                    blend,
                ),
            )
        });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("{}: {e}", stage.file));
        }
        Ok(Self {
            solid,
            translucent,
            flat,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vise_shader_api::{MANIFEST, Pack, StageKind, check};

    fn stage(material: &str) -> CheckedStage {
        let manifest = "schema = 1\nid = \"t\"\nname = \"T\"\nversion = \"1\"\napi = \"0.1\"\n\
            [material]\nfile = \"material.wgsl\"\n\
            [[options]]\nid = \"gain\"\nname = \"Gain\"\ntype = \"float\"\n\
            default = 1.0\nmin = 0.0\nmax = 2.0\n";
        let files = [(MANIFEST, manifest), ("material.wgsl", material)]
            .map(|(n, t)| (n.to_owned(), t.as_bytes().to_vec()))
            .into();
        let mut report = check(&Pack::from_files(files).unwrap()).unwrap();
        assert_eq!(report.stages[0].kind, StageKind::Material);
        report.stages.remove(0)
    }

    #[test]
    fn the_terrain_takes_a_prefix_and_validates_with_a_pack() {
        let terrain = vanilla_terrain();
        for name in [
            "Globals",
            "globals",
            "sprites_7",
            "sprite",
            "SPRITE_SPLIT",
            "block_slots",
            "sample_lightmap",
            "apply_fog",
        ] {
            assert!(terrain.contains(&format!("{PREFIX}{name}")), "{name}");
        }
        assert!(!terrain.contains("fs_sky"), "the sky stays out");
        let own = "override gain: f32;\nconst PI: f32 = 3.0;\n\
            fn sprite(c: vec4<f32>) -> vec4<f32> { return c * PI; }\n\
            fn apply_fog(c: vec4<f32>) -> vec4<f32> { return c * gain; }\n\
            fn material(input: MaterialInput) -> Surface {\n    \
                let lit = input.albedo * input.tint * vec4(input.lightmap, 1.0);\n    \
                return Surface(apply_fog(sprite(lit)));\n}\n";
        let stage = stage(own);
        let module = module(&stage).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&shader_source(&stage.source))));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        for entry in [
            "vise_material_vs",
            "vise_material_vs_flat",
            "vise_material_fs_solid",
            "vise_material_fs_translucent",
        ] {
            assert!(entries.contains(&entry), "{entry}");
        }
        drop(info);
        assert_eq!(
            present_constants(&module, &[("gain".into(), 1.5), ("other".into(), 1.0)]),
            [("gain", 1.5)]
        );
    }
}
