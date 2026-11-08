use crate::inputs::{Frame, MATERIAL_INPUT, POST_INPUT, SKY_INPUT, SURFACE, Struct};
use crate::package::Pack;

pub const GROUP: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageKind {
    Sky,
    Material,
    Post,
}

#[derive(Debug)]
pub struct Resource {
    pub binding: u32,
    pub name: &'static str,
    pub ty: &'static str,
    pub doc: &'static str,
}

const FRAME: Resource = Resource {
    binding: 0,
    name: "frame",
    ty: "var<uniform> frame: Frame",
    doc: "The frame (`Frame`).",
};

const POST_RESOURCES: [Resource; 6] = [
    FRAME,
    Resource {
        binding: 1,
        name: "scene_color",
        ty: "var scene_color: texture_2d<f32>",
        doc: "The world's picture before any pass.",
    },
    Resource {
        binding: 2,
        name: "scene_depth",
        ty: "var scene_depth: texture_depth_2d",
        doc: "The world's depth, reversed (1 near, 0 far and sky).",
    },
    Resource {
        binding: 3,
        name: "previous",
        ty: "var previous: texture_2d<f32>",
        doc: "The previous pass's output (`scene_color` for the first pass).",
    },
    Resource {
        binding: 4,
        name: "linear_sampler",
        ty: "var linear_sampler: sampler",
        doc: "Bilinear filtering, clamped to the edges.",
    },
    Resource {
        binding: 5,
        name: "nearest_sampler",
        ty: "var nearest_sampler: sampler",
        doc: "Nearest texel, clamped to the edges.",
    },
];

const POST_HELPERS: &str = "\
// The world's depth at `uv` (reversed: 0 is the sky).
fn depth_at(uv: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(scene_depth));
    let texel = vec2<i32>(clamp(uv * size, vec2<f32>(0.0), size - 1.0));
    return textureLoad(scene_depth, texel, 0);
}
// The camera-relative world position, in blocks, of the point at `uv` and `depth`.
fn view_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let view = frame.projection_inverse * clip;
    return (frame.view_inverse * vec4<f32>(view.xyz / view.w, 0.0)).xyz;
}
";

const POST_ENTRIES: &str = "\
struct VisePostOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex fn vise_vs_post(@builtin(vertex_index) i: u32) -> VisePostOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: VisePostOut;
    out.position = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(p.x, 1.0 - p.y);
    return out;
}
@fragment fn vise_fs_post(in: VisePostOut) -> @location(0) vec4<f32> {
    let color = textureSampleLevel(previous, linear_sampler, in.uv, 0.0);
    return post(PostInput(in.uv, in.position.xy, color, depth_at(in.uv)));
}
";

const MATERIAL_ENTRIES: &str = "\
struct ViseMaterialIn {
    @builtin(position) position: vec4<f32>,
    @location(0) albedo: vec4<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) light: vec2<f32>,
    @location(3) lightmap: vec3<f32>,
    @location(4) world: vec3<f32>,
    @location(5) uv: vec2<f32>,
    @location(6) @interpolate(flat) translucent: u32,
}
@fragment fn vise_fs_material(in: ViseMaterialIn) -> @location(0) vec4<f32> {
    let input = MaterialInput(in.albedo, in.tint, in.light, in.lightmap, in.world, in.uv, in.translucent != 0u);
    return material(input).color;
}
";

const SKY_ENTRIES: &str = "\
struct ViseSkyIn {
    @builtin(position) position: vec4<f32>,
    @location(0) direction: vec3<f32>,
    @location(1) color: vec4<f32>,
}
@fragment fn vise_fs_sky(in: ViseSkyIn) -> @location(0) vec4<f32> {
    return sky(SkyInput(normalize(in.direction), in.color));
}
";

impl StageKind {
    pub const ALL: [StageKind; 3] = [StageKind::Sky, StageKind::Material, StageKind::Post];

    pub fn name(self) -> &'static str {
        match self {
            StageKind::Sky => "sky",
            StageKind::Material => "material",
            StageKind::Post => "post",
        }
    }

    pub fn function(self) -> &'static str {
        self.name()
    }

    pub fn input(self) -> &'static Struct {
        match self {
            StageKind::Sky => &SKY_INPUT,
            StageKind::Material => &MATERIAL_INPUT,
            StageKind::Post => &POST_INPUT,
        }
    }

    pub fn output(self) -> &'static str {
        match self {
            StageKind::Material => SURFACE.name,
            StageKind::Sky | StageKind::Post => "vec4<f32>",
        }
    }

    pub fn signature(self) -> String {
        format!(
            "fn {}(input: {}) -> {}",
            self.function(),
            self.input().name,
            self.output()
        )
    }

    pub fn resources(self) -> &'static [Resource] {
        match self {
            StageKind::Post => &POST_RESOURCES,
            StageKind::Sky | StageKind::Material => std::slice::from_ref(&FRAME),
        }
    }

    pub fn prelude(self) -> String {
        let mut out =
            String::from("// Vise shader API prelude (docs/api/shaders-reference.md)\n");
        out += &Frame::WGSL.wgsl();
        out += &self.input().wgsl();
        if self == StageKind::Material {
            out += &SURFACE.wgsl();
        }
        for r in self.resources() {
            out += &format!("@group({GROUP}) @binding({}) {};\n", r.binding, r.ty);
        }
        if self == StageKind::Post {
            out += POST_HELPERS;
        }
        out
    }

    pub fn entries(self) -> &'static str {
        match self {
            StageKind::Sky => SKY_ENTRIES,
            StageKind::Material => MATERIAL_ENTRIES,
            StageKind::Post => POST_ENTRIES,
        }
    }
}

#[derive(Debug)]
pub struct Composed {
    pub text: String,
    pub parts: Vec<(String, std::ops::Range<usize>)>,
}

impl Composed {
    pub fn locate(&self, offset: usize) -> (&str, usize, usize) {
        let (file, range) = self
            .parts
            .iter()
            .find(|(_, r)| r.contains(&offset))
            .or(self.parts.last())
            .expect("a composed stage has parts");
        let before = &self.text[range.start..offset.clamp(range.start, range.end)];
        let line = before.matches('\n').count() + 1;
        let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
        (file, line, column)
    }
}

pub fn compose(pack: &Pack, kind: StageKind, file: &str) -> Composed {
    let mut composed = Composed {
        text: String::new(),
        parts: Vec::new(),
    };
    let mut push = |name: &str, text: &str| {
        let start = composed.text.len();
        composed.text += text;
        if !text.ends_with('\n') {
            composed.text.push('\n');
        }
        composed
            .parts
            .push((name.to_owned(), start..composed.text.len()));
    };
    push("<prelude>", &kind.prelude());
    for shared in pack.manifest.shared.iter().filter(|s| *s != file) {
        push(shared, pack.source(shared).unwrap_or_default());
    }
    push(file, pack.source(file).unwrap_or_default());
    push("<entries>", kind.entries());
    composed
}
