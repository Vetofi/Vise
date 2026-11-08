
use std::collections::HashMap;

use vise_shaders::{Format, Group, Kind, Post, Val};

use super::pack_draw::DrawUniforms;
use super::pack_entities::{PROGRAMS, PackEntities};
use super::pack_sky::PackSky;
use super::pack_uniforms::{self, FrameUniforms, PackInputs};

const BUFFERS: usize = vise_shaders::BUFFERS;

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Color(usize),
    Depth,
    Depth1,
    Noise,
    Dummy,
    Shadow,
    Atlas,
    Lightmap,
    ShadowDepth(usize, bool),
    ShadowColor(usize),
}

#[derive(Clone)]
struct Binding {
    source: Source,
    binding: u32,
    filtering: bool,
    sampler: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SamplerKind {
    Linear,
    Nearest,
    Repeat,
    NearestRepeat,
    Comparison,
}

impl SamplerKind {
    fn of(source: Source, filtering: bool) -> Self {
        match source {
            Source::Color(_) | Source::ShadowColor(_) | Source::Dummy if !filtering => {
                Self::Nearest
            }
            Source::Depth | Source::Depth1 | Source::ShadowDepth(_, false) => Self::Nearest,
            Source::Noise => Self::Repeat,
            Source::Shadow | Source::ShadowDepth(_, true) => Self::Comparison,
            Source::Atlas => Self::NearestRepeat,
            Source::Color(_) | Source::ShadowColor(_) | Source::Dummy | Source::Lightmap => {
                Self::Linear
            }
        }
    }

    fn binding_type(self) -> wgpu::SamplerBindingType {
        match self {
            Self::Linear | Self::Repeat => wgpu::SamplerBindingType::Filtering,
            Self::Nearest | Self::NearestRepeat => wgpu::SamplerBindingType::NonFiltering,
            Self::Comparison => wgpu::SamplerBindingType::Comparison,
        }
    }
}

fn share_samplers(module: &mut naga::Module, slot: &HashMap<u32, u32>) {
    let mut first = HashMap::new();
    let mut same = HashMap::new();
    for (h, g) in module.global_variables.iter_mut() {
        let Some(b) = g.binding.as_mut() else {
            continue;
        };
        if !matches!(module.types[g.ty].inner, naga::TypeInner::Sampler { .. }) {
            continue;
        }
        let Some(&to) = slot.get(&b.binding) else {
            continue;
        };
        b.binding = to;
        match first.entry(to) {
            std::collections::hash_map::Entry::Occupied(e) => _ = same.insert(h, *e.get()),
            std::collections::hash_map::Entry::Vacant(e) => _ = e.insert(h),
        }
    }
    if same.is_empty() {
        return;
    }
    let fix = |expressions: &mut naga::Arena<naga::Expression>| {
        for (_, e) in expressions.iter_mut() {
            if let naga::Expression::GlobalVariable(g) = e
                && let Some(&to) = same.get(g)
            {
                *g = to;
            }
        }
    };
    for (_, f) in module.functions.iter_mut() {
        fix(&mut f.expressions);
    }
    for ep in &mut module.entry_points {
        fix(&mut ep.function.expressions);
    }
}

struct Sky {
    geometry: PackSky,
    basic: Option<Pass>,
    textured: Option<Pass>,
}

struct Entities {
    geometry: PackEntities,
    passes: [Option<Pass>; PROGRAMS],
    late: [bool; PROGRAMS],
    shadow: [Option<Pass>; 2],
    only: (bool, bool),
}

impl Entities {
    fn drawn(&self) -> bool {
        self.passes.iter().any(Option::is_some)
    }
}

#[derive(Clone)]
struct Pass {
    name: String,
    group: Group,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    ubo: wgpu::Buffer,
    block: Vec<u8>,
    uniforms: Vec<(Option<usize>, String, u32)>,
    samplers: Vec<Binding>,
    writes: Vec<(u32, usize)>,
    flip: u16,
    reads: u16,
    shadow_reads: u16,
    written: u16,
    mipmaps: u16,
    draw: bool,
    atlas: Option<wgpu::TextureView>,
    ids: bool,
    groups: HashMap<u64, wgpu::BindGroup>,
}

struct Mips {
    all: wgpu::TextureView,
    levels: Vec<(wgpu::TextureView, wgpu::BindGroup)>,
}

struct Targets {
    size: [u32; 2],
    color: [Option<[wgpu::TextureView; 2]>; BUFFERS],
    depth: wgpu::TextureView,
    depth_texture: wgpu::Texture,
    depth1: Option<(wgpu::Texture, wgpu::TextureView)>,
    mips: [Option<[Mips; 2]>; BUFFERS],
}

pub(super) struct World<'a> {
    pub atlas: Option<(wgpu::TextureView, [u32; 2])>,
    pub lightmap: wgpu::TextureView,
    pub terrain: Option<Geometry<'a>>,
    pub block_ids: &'a [i32],
    pub celestial: Option<wgpu::TextureView>,
    pub clouds: Option<&'a wgpu::BindGroupLayout>,
    pub weather: bool,
    pub hand: bool,
}

pub(super) struct Clouds {
    pub vertices: wgpu::Buffer,
    pub group: wgpu::BindGroup,
    pub indices: wgpu::Buffer,
    pub quads: u32,
}

pub(super) struct Weather {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub quads: [std::ops::Range<u32>; 2],
    pub sprites: [Option<wgpu::TextureView>; 2],
    pub generation: u64,
}

pub struct PackPost {
    pub notes: Vec<String>,
    passes: Vec<Pass>,
    terrain: Option<Pass>,
    water: Option<Pass>,
    sky: Option<Sky>,
    entities: Option<Entities>,
    clouds: Option<Pass>,
    cloud_draw: Option<Clouds>,
    hides_clouds: bool,
    weather: Option<[Pass; 2]>,
    weather_draw: Option<Weather>,
    weather_generation: u64,
    hand: Option<(Pass, super::pack_hand::PackHand)>,
    depth1: bool,
    shadow_targets: Option<super::pack_shadow::ShadowTargets>,
    shadow_terrain: Option<Pass>,
    shadow_water: Option<Pass>,
    shadow_comp: Vec<Pass>,
    mipped: u16,
    mips_valid: u16,
    down: HashMap<wgpu::TextureFormat, (wgpu::RenderPipeline, wgpu::BindGroupLayout)>,
    draws: DrawUniforms,
    block_ids: Option<(wgpu::BindGroupLayout, wgpu::Buffer)>,
    page_groups: HashMap<u32, (wgpu::Buffer, wgpu::BindGroup)>,
    pub timing: Option<super::pack_timing::PassTiming>,
    custom: vise_shaders::Custom,
    atlas: Option<wgpu::TextureView>,
    lightmap: wgpu::TextureView,
    deferred_pre: u16,
    composite_pre: u16,
    formats: [wgpu::TextureFormat; BUFFERS],
    clears: [Option<[f32; 4]>; BUFFERS],
    used: u16,
    main: u16,
    targets: Option<Targets>,
    noise: wgpu::TextureView,
    dummy: wgpu::TextureView,
    shadow: (wgpu::TextureView, wgpu::Sampler),
    shadow_cleared: bool,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    repeat: wgpu::Sampler,
    nearest_repeat: wgpu::Sampler,
    blit: (wgpu::RenderPipeline, wgpu::BindGroupLayout),
    blit_depth: bool,
    blit_clears: Vec<usize>,
    other_clears: Vec<usize>,
    blit_group: Option<([u32; 2], wgpu::BindGroup)>,
    copy: (wgpu::RenderPipeline, wgpu::BindGroupLayout),
    uniforms: FrameUniforms,
}

fn texture_format(f: Format, features: wgpu::Features) -> wgpu::TextureFormat {
    use wgpu::TextureFormat as T;
    let pick = |r, rg, rgba| match f.channels {
        1 => r,
        2 => rg,
        _ => rgba,
    };
    let half = pick(T::R16Float, T::Rg16Float, T::Rgba16Float);
    match (f.kind, f.bits) {
        (Kind::Unorm, 10) => T::Rgb10a2Unorm,
        (Kind::Unorm, 16)
            if features.contains(
                wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
                    | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
            ) =>
        {
            pick(T::R16Unorm, T::Rg16Unorm, T::Rgba16Unorm)
        }
        (Kind::Unorm, b) if b <= 8 => pick(T::R8Unorm, T::Rg8Unorm, T::Rgba8Unorm),
        (Kind::Float, 32) => pick(T::R32Float, T::Rg32Float, T::Rgba32Float),
        (Kind::Float, 11) if features.contains(wgpu::Features::RG11B10UFLOAT_RENDERABLE) => {
            T::Rg11b10Ufloat
        }
        (Kind::Int, 8) => pick(T::R8Sint, T::Rg8Sint, T::Rgba8Sint),
        (Kind::Int, 16) => pick(T::R16Sint, T::Rg16Sint, T::Rgba16Sint),
        (Kind::Int, _) => pick(T::R32Sint, T::Rg32Sint, T::Rgba32Sint),
        (Kind::Uint, 8) => pick(T::R8Uint, T::Rg8Uint, T::Rgba8Uint),
        (Kind::Uint, 10) => T::Rgb10a2Uint,
        (Kind::Uint, 16) => pick(T::R16Uint, T::Rg16Uint, T::Rgba16Uint),
        (Kind::Uint, _) => pick(T::R32Uint, T::Rg32Uint, T::Rgba32Uint),
        _ => half,
    }
}

fn source(name: &str, shadow_space: bool, shadow_pass: bool, watershadow: bool) -> Source {
    let shadow_color = match name {
        "shadowcolor" | "shadowcolor0" => Some(0),
        "shadowcolor1" => Some(1),
        _ => None,
    };
    let shadow_depth = match name {
        "shadowtex0" | "watershadow" | "shadowtex0HW" => Some(0),
        "shadowtex1" | "shadowtex1HW" => Some(1),
        "shadow" => Some(usize::from(watershadow)),
        _ => None,
    };
    if shadow_pass {
        if let Some(n) = shadow_depth {
            return Source::ShadowDepth(n, name.ends_with("HW"));
        }
        if let Some(n) = shadow_color {
            return if shadow_space {
                Source::Color(n)
            } else {
                Source::ShadowColor(n)
            };
        }
    }
    if shadow_space && vise_shaders::buffer_index(name).is_some() {
        return Source::Dummy;
    }
    match name {
        "depthtex0" | "gdepthtex" => Source::Depth,
        "depthtex1" | "depthtex2" => Source::Depth1,
        "noisetex" => Source::Noise,
        "texture" | "gtexture" | "tex" => Source::Atlas,
        "lightmap" => Source::Lightmap,
        n => vise_shaders::buffer_index(n).map_or(Source::Dummy, Source::Color),
    }
}

fn noise(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let texels: Vec<u8> = (0..256 * 256 * 4)
        .map(|i| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            if i % 4 == 3 { 255 } else { (state >> 56) as u8 }
        })
        .collect();
    texture(device, queue, "Shader pack noisetex", 256, &texels)
}

fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    side: u32,
    texels: &[u8],
) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: side,
        height: side,
        depth_or_array_layers: 1,
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        tex.as_image_copy(),
        texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(side * 4),
            rows_per_image: None,
        },
        size,
    );
    tex.create_view(&Default::default())
}

fn sampler(
    device: &wgpu::Device,
    filter: wgpu::FilterMode,
    address: wgpu::AddressMode,
) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Shader pack"),
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: filter,
        min_filter: filter,
        ..Default::default()
    })
}

fn checked<T>(device: &wgpu::Device, create: impl FnOnce() -> T) -> Result<T, String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let value = crate::crash::contain(create);
    match pollster::block_on(scope.pop()) {
        Some(e) => {
            let mut message = match &e {
                wgpu::Error::Validation { source, .. } => format!("{source:?}"),
                e => e.to_string(),
            };
            let mut source = std::error::Error::source(&e);
            while let Some(s) = source {
                message = format!("{message}: {s}");
                source = s.source();
            }
            Err(message)
        }
        None => value.map_err(|panic| format!("the shader compiler failed: {panic}")),
    }
}

fn helper(
    device: &wgpu::Device,
    vertex: &str,
    entry: &str,
    entries: &[wgpu::BindGroupLayoutEntry],
    targets: &[Option<wgpu::ColorTargetState>],
    depth: Option<wgpu::DepthStencilState>,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shader pack helpers"),
        source: wgpu::ShaderSource::Wgsl(include_str!("pack_post.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(entry),
        entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(entry),
        bind_group_layouts: &[Some(&layout)],
        ..Default::default()
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: depth,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets,
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, layout)
}

fn entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty,
        count: None,
    }
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    entry(
        binding,
        wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
    )
}

fn blend_state(b: vise_shaders::Blend) -> wgpu::BlendState {
    use vise_shaders::Factor as F;
    use wgpu::BlendFactor as W;
    let f = |f| match f {
        F::Zero => W::Zero,
        F::One => W::One,
        F::SrcColor => W::Src,
        F::OneMinusSrcColor => W::OneMinusSrc,
        F::DstColor => W::Dst,
        F::OneMinusDstColor => W::OneMinusDst,
        F::SrcAlpha => W::SrcAlpha,
        F::OneMinusSrcAlpha => W::OneMinusSrcAlpha,
        F::DstAlpha => W::DstAlpha,
        F::OneMinusDstAlpha => W::OneMinusDstAlpha,
        F::SrcAlphaSaturate => W::SrcAlphaSaturated,
    };
    let component = |src, dst| wgpu::BlendComponent {
        src_factor: f(src),
        dst_factor: f(dst),
        operation: wgpu::BlendOperation::Add,
    };
    wgpu::BlendState {
        color: component(b[0], b[1]),
        alpha: component(b[2], b[3]),
    }
}

fn target(format: wgpu::TextureFormat) -> Option<wgpu::ColorTargetState> {
    Some(wgpu::ColorTargetState {
        format,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })
}

fn attach(
    view: &wgpu::TextureView,
    ops: wgpu::Operations<wgpu::Color>,
) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment {
        view,
        resolve_target: None,
        depth_slice: None,
        ops,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TerrainList {
    Solid,
    Translucent,
    ShadowSolid,
    ShadowTranslucent,
}

impl TerrainList {
    fn translucent(self) -> bool {
        matches!(self, Self::Translucent | Self::ShadowTranslucent)
    }
}

pub(super) type TerrainDraw<'a> =
    dyn FnMut(&mut wgpu::RenderPass<'_>, TerrainList, &dyn Fn(&mut wgpu::RenderPass<'_>, u32)) + 'a;

pub(super) struct ShadowView {
    pub cull: glam::Mat4,
    pub light: glam::Vec3,
    pub distance: Option<f32>,
}

fn block_ids(device: &wgpu::Device, table: &[i32]) -> (wgpu::BindGroupLayout, wgpu::Buffer) {
    let empty = [-1, -1];
    let table = if table.is_empty() { &empty[..] } else { table };
    let buffer = wgpu::util::DeviceExt::create_buffer_init(
        device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("Shader pack block ids"),
            contents: bytemuck::cast_slice(table),
            usage: wgpu::BufferUsages::STORAGE,
        },
    );
    let storage = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Shader pack terrain"),
        entries: &[storage(0), storage(1)],
    });
    (layout, buffer)
}

pub(super) struct Geometry<'a> {
    pub buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    pub table: Option<&'a wgpu::BindGroupLayout>,
    pub depth_write: bool,
    pub atlas: Option<wgpu::TextureView>,
    pub ids: Option<&'a wgpu::BindGroupLayout>,
    pub cull: bool,
}

#[allow(clippy::too_many_arguments)]
fn build_pass(
    device: &wgpu::Device,
    p: vise_shaders::PostProgram,
    formats: &[wgpu::TextureFormat; BUFFERS],
    shadow: Option<&[wgpu::TextureFormat; 2]>,
    screen: wgpu::TextureFormat,
    notes: &mut Vec<String>,
    geometry: Option<&Geometry<'_>>,
    custom: &[String],
    draws: &wgpu::BindGroupLayout,
) -> Option<Pass> {
    let features = device.features();
    let index = |name: &str| {
        pack_uniforms::index(name).or_else(|| {
            let k = custom.iter().position(|c| c == name)?;
            Some(pack_uniforms::NAMES.len() + k)
        })
    };
    let mask = |flips: &[(u32, bool)]| flips.iter().filter(|f| f.1).fold(0u16, |m, f| m | 1 << f.0);
    let interface = &p.program.interface;
    let draw = interface.draw;
    let mut entries = vec![entry(
        0,
        wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
    )];
    let mut samplers = Vec::new();
    let (mut kinds, mut slots) = (HashMap::new(), HashMap::new());
    let mut reads = 0u16;
    let mut shadow_reads = 0u16;
    let watershadow = interface.samplers.iter().any(|s| s.name == "watershadow");
    let in_place = match geometry {
        Some(_) => p.buffers.iter().fold(0u16, |m, &b| m | 1 << b),
        None => 0,
    };
    for s in &interface.samplers {
        let mut src = source(&s.name, p.group.shadow(), shadow.is_some(), watershadow);
        let blank = match src {
            Source::ShadowDepth(..) => p.group == Group::Shadow,
            Source::Atlas => geometry.is_none(),
            Source::Depth => geometry.is_some(),
            Source::Depth1 => p.group == Group::Gbuffers,
            Source::Color(n) => in_place & 1 << n != 0,
            _ => false,
        };
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let mut sample_type = match src {
            Source::Color(n) => formats[n]
                .sample_type(None, Some(features))
                .unwrap_or(float),
            Source::ShadowColor(n) => shadow
                .and_then(|f| f[n].sample_type(None, Some(features)))
                .unwrap_or(float),
            Source::Depth | Source::Depth1 | Source::ShadowDepth(..) => {
                wgpu::TextureSampleType::Float { filterable: false }
            }
            Source::Lightmap => float,
            _ => float,
        };
        let int = !matches!(sample_type, wgpu::TextureSampleType::Float { .. });
        if s.ty == "sampler2DShadow" {
            src = match src {
                Source::ShadowDepth(n, _) => Source::ShadowDepth(n, true),
                _ => Source::Shadow,
            };
            sample_type = wgpu::TextureSampleType::Depth;
        }
        let wanted = match s.ty.as_str() {
            "sampler2DShadow" => true,
            "sampler2D" => !int,
            "isampler2D" => sample_type == wgpu::TextureSampleType::Sint,
            "usampler2D" => sample_type == wgpu::TextureSampleType::Uint,
            _ => false,
        };
        if !wanted {
            notes.push(format!(
                "{}: `{} {}` is not provided yet",
                p.name, s.ty, s.name
            ));
            (src, sample_type) = (Source::Dummy, float);
        }
        if blank && sample_type == wgpu::TextureSampleType::Depth {
            src = Source::Shadow;
        } else if blank {
            src = Source::Dummy;
            if int {
                sample_type = float;
            }
        }
        let filtering = sample_type == float;
        match src {
            Source::Color(n) => reads |= 1 << n,
            Source::ShadowColor(n) => shadow_reads |= 1 << n,
            _ => {}
        }
        entries.push(texture_entry(s.binding, sample_type));
        let kind = SamplerKind::of(src, filtering);
        let sampler = *kinds.entry(kind).or_insert_with(|| {
            entries.push(entry(
                s.binding + 1,
                wgpu::BindingType::Sampler(kind.binding_type()),
            ));
            s.binding + 1
        });
        slots.insert(s.binding + 1, sampler);
        samplers.push(Binding {
            source: src,
            binding: s.binding,
            filtering,
            sampler,
        });
    }
    let final_pass = p.group == Group::Final;
    let mut targets = Vec::new();
    let mut writes = Vec::new();
    for &location in &interface.outputs {
        let l = location as usize;
        if targets.len() <= l {
            targets.resize(l + 1, None);
        }
        if final_pass {
            if location == 0 {
                targets[0] = target(screen);
            }
        } else if let Some(&buffer) = p.buffers.get(l) {
            let format = formats[buffer as usize];
            let mut state = target(format);
            if let (Some(t), Some(b)) = (&mut state, p.blend)
                && format
                    .guaranteed_format_features(features)
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
            {
                t.blend = Some(blend_state(b));
            }
            targets[l] = state;
            writes.push((location, buffer as usize));
        }
    }
    if !final_pass && writes.is_empty() {
        notes.push(format!("{}: writes no buffer; skipped", p.name));
        return None;
    }
    let written = writes.iter().fold(0u16, |m, w| m | 1 << w.1);
    let kept = p
        .flips
        .iter()
        .filter(|f| !f.1)
        .fold(0u16, |m, f| m | 1 << f.0);
    let flip = if final_pass || geometry.is_some() {
        0
    } else {
        (written & !kept) | mask(&p.flips)
    };
    let created = checked(device, || {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&p.name),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&p.name),
            bind_group_layouts: &[
                Some(&layout),
                geometry.and_then(|g| g.table),
                draw.then_some(draws),
                geometry.and_then(|g| g.ids),
            ],
            ..Default::default()
        });
        let module = |mut m: naga::Module| {
            share_samplers(&mut m, &slots);
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&p.name),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(m)),
            })
        };
        let (vs, fs) = (module(p.program.vertex), module(p.program.fragment));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&p.name),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                buffers: geometry.map_or(&[], |g| g.buffers),
            },
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Cw,
                cull_mode: geometry.filter(|g| g.cull).map(|_| wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: geometry.map(|g| wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(g.depth_write),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &fs,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        (layout, pipeline)
    });
    let (layout, pipeline) = match created {
        Ok(created) => created,
        Err(e) => {
            notes.push(format!("{}: skipped: {e}", p.name));
            return None;
        }
    };
    let interface = &p.program.interface;
    let missing: Vec<&str> = interface
        .uniforms
        .iter()
        .filter(|u| !u.name.starts_with("hz_") && index(&u.name).is_none())
        .map(|u| u.name.as_str())
        .collect();
    if !missing.is_empty() {
        notes.push(format!("{}: zero for now: {}", p.name, missing.join(" ")));
    }
    Some(Pass {
        ubo: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&p.name),
            size: u64::from(interface.uniform_size.max(16)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        block: vec![0; interface.uniform_size as usize],
        uniforms: interface
            .uniforms
            .iter()
            .map(|u| (index(&u.name), u.ty.clone(), u.offset))
            .collect(),
        name: p.name,
        group: p.group,
        pipeline,
        layout,
        samplers,
        writes,
        flip,
        reads,
        shadow_reads,
        written,
        mipmaps: p.mipmaps,
        draw,
        atlas: geometry.and_then(|g| g.atlas.clone()),
        ids: geometry.is_some_and(|g| g.ids.is_some()),
        groups: HashMap::new(),
    })
}

impl PackPost {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        post: Post,
        screen: wgpu::TextureFormat,
        world: World<'_>,
    ) -> Self {
        let features = device.features();
        let formats: [wgpu::TextureFormat; BUFFERS] =
            std::array::from_fn(|i| texture_format(post.config.buffers[i].format, features));
        let clears = std::array::from_fn(|i| {
            let b = &post.config.buffers[i];
            b.clear.then(|| {
                b.clear_color
                    .unwrap_or(if i == 1 { [1.0; 4] } else { [0.0; 4] })
            })
        });
        let mut notes = post.notes;
        notes.extend(post.config.notes);
        let shadow_config = post.config.shadow.clone();
        let mut custom = post.custom;
        let custom_names: Vec<String> = custom.uniforms().map(|u| u.0.to_owned()).collect();
        custom.eval(0.0, &|n| pack_uniforms::index(n).map(|_| Val::scalar(0.0)));
        if !custom.notes.is_empty() {
            notes.push(format!("custom uniforms: {}", custom.notes.join("; ")));
            custom.notes.clear();
        }
        let mask =
            |flips: &[(u32, bool)]| flips.iter().filter(|f| f.1).fold(0u16, |m, f| m | 1 << f.0);
        let mut used = 1u16;
        let draws = DrawUniforms::new(device);
        let shadow_formats: Option<[wgpu::TextureFormat; 2]> = post.shadow_pass.then(|| {
            std::array::from_fn(|n| texture_format(shadow_config.colors[n].format, features))
        });
        let shadow_space: [wgpu::TextureFormat; BUFFERS] = std::array::from_fn(|i| {
            shadow_formats
                .filter(|_| i < 2)
                .map_or(formats[i], |f| f[i])
        });
        let mut build = |p: vise_shaders::PostProgram, geometry: Option<&Geometry<'_>>| {
            let in_shadow = p.group.shadow();
            let pass = build_pass(
                device,
                p,
                if in_shadow { &shadow_space } else { &formats },
                shadow_formats.as_ref(),
                screen,
                &mut notes,
                geometry,
                &custom_names,
                &draws.layout,
            )?;
            if !in_shadow {
                used |= pass.reads | pass.written | pass.flip;
            }
            Some(pass)
        };
        let block_ids = world
            .terrain
            .as_ref()
            .filter(|g| g.table.is_some())
            .map(|_| block_ids(device, world.block_ids));
        let terrain_geometry = world.terrain.as_ref().map(|g| Geometry {
            buffers: g.buffers,
            table: g.table,
            depth_write: g.depth_write,
            atlas: g.atlas.clone(),
            ids: block_ids.as_ref().map(|i| &i.0),
            cull: g.cull,
        });
        let terrain = post
            .terrain
            .zip(terrain_geometry.as_ref())
            .and_then(|(p, g)| build(p, Some(g)));
        let water = post
            .water
            .zip(terrain_geometry.as_ref())
            .and_then(|(p, g)| build(p, Some(g)));
        let shadow_terrain = post
            .shadow
            .zip(terrain_geometry.as_ref())
            .and_then(|(p, g)| build(p, Some(g)));
        let shadow_water = post
            .shadow_water
            .zip(terrain_geometry.as_ref())
            .and_then(|(p, g)| build(p, Some(g)));
        let sky = (post.sky.is_some() || post.sky_textured.is_some()).then(|| {
            let geometry = PackSky::new(device);
            let basic = Geometry {
                buffers: &PackSky::buffers(),
                table: Some(&geometry.layout),
                depth_write: false,
                atlas: None,
                ids: None,
                cull: false,
            };
            let basic = post.sky.and_then(|p| build(p, Some(&basic)));
            let textured = Geometry {
                buffers: &PackSky::celestial_buffers(),
                table: None,
                depth_write: false,
                atlas: world.celestial.clone(),
                ids: None,
                cull: false,
            };
            let textured = post.sky_textured.and_then(|p| build(p, Some(&textured)));
            Sky {
                geometry,
                basic,
                textured,
            }
        });
        let sky = sky.filter(|s| s.basic.is_some() || s.textured.is_some());
        let programs = [
            post.entities,
            post.block,
            post.entities_translucent,
            post.particles,
        ];
        let late = [false, false, post.translucent_late, post.particles_late];
        let shadow_only = (post.shadow_only_player, post.shadow_only_lit_blocks);
        let shadow_programs = [post.shadow_entities, post.shadow_block];
        let entity_vertices = (programs.iter().chain(&shadow_programs))
            .any(Option::is_some)
            .then(|| PackEntities::new(device, &draws.layout));
        let entity_buffers = super::pack_entities::buffers();
        let entity_geometry = entity_vertices.as_ref().map(|e| Geometry {
            buffers: &entity_buffers,
            table: Some(&e.table),
            depth_write: true,
            atlas: world.celestial.clone(),
            ids: None,
            cull: false,
        });
        let entity_passes = programs.map(|p| {
            p.zip(entity_geometry.as_ref())
                .and_then(|(p, g)| build(p, Some(g)))
        });
        let shadow_entities = shadow_programs.map(|p| {
            p.zip(entity_geometry.as_ref())
                .and_then(|(p, g)| build(p, Some(g)))
        });
        let entities = entity_vertices
            .filter(|_| (entity_passes.iter().chain(&shadow_entities)).any(Option::is_some))
            .map(|geometry| Entities {
                geometry,
                passes: entity_passes,
                late,
                shadow: shadow_entities,
                only: shadow_only,
            });
        let cloud_geometry = world.clouds.map(|layout| Geometry {
            buffers: &entity_buffers,
            table: Some(layout),
            depth_write: true,
            atlas: world.celestial.clone(),
            ids: None,
            cull: false,
        });
        let hides_clouds = post.clouds.is_some() && cloud_geometry.is_some();
        let weather_geometry = world.weather.then(|| Geometry {
            buffers: &entity_buffers,
            table: None,
            depth_write: true,
            atlas: None,
            ids: None,
            cull: false,
        });
        let hand_geometry = world.hand.then(|| super::pack_hand::PackHand::new(device));
        let hand = post.hand.zip(hand_geometry).and_then(|(p, geometry)| {
            let g = Geometry {
                buffers: &entity_buffers,
                table: Some(&geometry.layout),
                depth_write: true,
                atlas: world.celestial.clone(),
                ids: None,
                cull: false,
            };
            build(p, Some(&g)).map(|pass| (pass, geometry))
        });
        let weather = post
            .weather
            .zip(weather_geometry.as_ref())
            .and_then(|(p, g)| build(p, Some(g)))
            .map(|rain| {
                let mut snow = rain.clone();
                snow.name = format!("{} (snow)", rain.name);
                [rain, snow]
            });
        let clouds = post
            .clouds
            .zip(cloud_geometry.as_ref())
            .and_then(|(p, g)| build(p, Some(g)));
        let skip = std::env::var("VISE_SHADER_PACK_SKIP").unwrap_or_default();
        let passes: Vec<Pass> = post
            .programs
            .into_iter()
            .filter(|p| !skip.split(',').any(|s| s == p.name))
            .filter_map(|p| build(p, None))
            .collect();
        let shadow_comp: Vec<Pass> = post
            .shadow_comp
            .into_iter()
            .filter(|p| !skip.split(',').any(|s| s == p.name))
            .filter_map(|p| build(p, None))
            .collect();
        if let Some(t) = &terrain {
            notes.push(format!("terrain draws with {}", t.name));
        }
        for (part, pass) in [
            ("terrain", &shadow_terrain),
            ("translucent terrain", &shadow_water),
        ] {
            if let Some(p) = pass {
                notes.push(format!("{part} casts shadows with {}", p.name));
            }
        }
        if let Some(w) = &water {
            notes.push(format!("translucent terrain draws with {}", w.name));
        }
        let mut shadow_water = shadow_water;
        if let Some(p) = &mut shadow_water {
            p.name.push_str(" (translucent)");
        }
        for (part, pass) in sky
            .iter()
            .flat_map(|s| [("sky", &s.basic), ("sun and moon", &s.textured)])
        {
            if let Some(p) = pass {
                notes.push(format!("the {part} draw with {}", p.name));
            }
        }
        let parts = [
            "entities",
            "block entities",
            "translucent entities",
            "particles",
        ];
        if let Some(c) = &clouds {
            notes.push(format!("clouds draw with {}", c.name));
        }
        if let Some((h, _)) = &hand {
            notes.push(format!("the hand draws with {}", h.name));
        }
        if let Some([w, _]) = &weather {
            notes.push(format!("rain and snow draw with {}", w.name));
        }
        for (part, pass) in entities.iter().flat_map(|e| parts.iter().zip(&e.passes)) {
            if let Some(p) = pass {
                notes.push(format!("{part} draw with {}", p.name));
            }
        }
        let mut entities = entities;
        for e in entities.iter_mut() {
            let only = [
                (e.only.0, " (only the player)"),
                (e.only.1, " (only those that emit light)"),
            ];
            for ((part, pass), (only, which)) in parts.iter().zip(&mut e.shadow).zip(only) {
                if let Some(p) = pass {
                    let which = if only { which } else { "" };
                    notes.push(format!("{part}{which} cast shadows with {}", p.name));
                    p.name.push_str(&format!(" ({part})"));
                }
            }
        }
        let depth1 = water.is_some()
            && passes
                .iter()
                .filter(|p| matches!(p.group, Group::Composite | Group::Final))
                .chain(&water)
                .chain(weather.iter().take(1).flatten())
                .any(|p| p.samplers.iter().any(|s| s.source == Source::Depth1));
        let every_pass = || {
            (passes
                .iter()
                .chain(&shadow_comp)
                .chain(&terrain)
                .chain(&water))
            .chain(shadow_terrain.iter().chain(&shadow_water))
            .chain(sky.iter().flat_map(|s| s.basic.iter().chain(&s.textured)))
            .chain(entities.iter().flat_map(|e| e.passes.iter().flatten()))
            .chain(entities.iter().flat_map(|e| e.shadow.iter().flatten()))
            .chain(&clouds)
            .chain(weather.iter().flatten())
            .chain(hand.iter().map(|h| &h.0))
        };
        let shadow_targets = shadow_formats.map(|f| {
            let colors = every_pass().fold(0u16, |m, p| {
                let own = if p.group.shadow() {
                    p.reads | p.written | p.flip
                } else {
                    0
                };
                m | own | p.shadow_reads
            });
            let depth1 = every_pass()
                .flat_map(|p| &p.samplers)
                .any(|s| matches!(s.source, Source::ShadowDepth(1, _)));
            let t =
                super::pack_shadow::ShadowTargets::new(device, &shadow_config, f, colors, depth1);
            notes.push(format!(
                "shadow map {0}x{0} over {2} blocks, sunPathRotation {3}, shadowcolor buffers {colors:#b}{1}",
                t.size,
                if depth1 { ", shadowtex1" } else { "" },
                shadow_config.distance,
                shadow_config.sun_path_rotation,
            ));
            t
        });
        let mut cleared = Vec::new();
        for b in 1..BUFFERS {
            let bit = 1u16 << b;
            let touched = |p: &&Pass| (p.reads | p.flip | p.written) & bit != 0;
            let first = sky
                .iter()
                .flat_map(|s| s.basic.iter().chain(&s.textured))
                .chain(&terrain)
                .chain(entities.iter().flat_map(|e| e.passes.iter().flatten()))
                .chain(&clouds)
                .chain(&water)
                .chain(weather.iter().take(1).flatten())
                .chain(hand.iter().map(|h| &h.0))
                .find(touched)
                .map(|_| true)
                .or_else(|| {
                    let p = passes.iter().find(touched)?;
                    Some(p.reads & bit != 0 || p.written & bit == 0)
                });
            if clears[b].is_some() && first == Some(true) {
                cleared.push(b);
            }
        }
        let blit_depth = terrain.is_some()
            || water.is_some()
            || sky.is_some()
            || entities.as_ref().is_some_and(Entities::drawn)
            || clouds.is_some()
            || weather.is_some()
            || hand.is_some()
            || passes.iter().any(|p| {
                p.samplers
                    .iter()
                    .any(|s| matches!(s.source, Source::Depth | Source::Depth1))
            });
        let room = 8 - 1;
        let other_clears = cleared.split_off(room.min(cleared.len()));
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let blit_targets = std::iter::once(target(formats[0]));
        let blit_targets: Vec<_> = blit_targets
            .chain(cleared.iter().map(|&b| {
                Some(wgpu::ColorTargetState {
                    format: formats[b],
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })
            }))
            .collect();
        let blit = helper(
            device,
            "vs_full",
            if blit_depth {
                "fs_blit"
            } else {
                "fs_blit_color"
            },
            &[
                texture_entry(0, float),
                texture_entry(1, wgpu::TextureSampleType::Depth),
            ],
            &blit_targets,
            blit_depth.then_some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
        );
        let copy = helper(
            device,
            "vs_uv",
            "fs_copy",
            &[
                texture_entry(0, float),
                entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
            &[target(screen)],
            None,
        );
        let filterable = |b: usize| {
            formats[b].sample_type(None, Some(device.features()))
                == Some(wgpu::TextureSampleType::Float { filterable: true })
        };
        let asked = passes.iter().fold(0u16, |m, p| m | p.mipmaps);
        let mipped = (0..BUFFERS)
            .filter(|&b| asked & 1 << b != 0 && filterable(b))
            .fold(0u16, |m, b| m | 1 << b);
        if asked & !mipped != 0 {
            notes.push(format!(
                "mipmaps of unfilterable buffers are not generated (buffers {:#b})",
                asked & !mipped
            ));
        }
        let mut down = HashMap::new();
        for b in (0..BUFFERS).filter(|b| mipped & 1 << b != 0) {
            down.entry(formats[b]).or_insert_with(|| {
                helper(
                    device,
                    "vs_full",
                    "fs_down",
                    &[
                        texture_entry(0, float),
                        entry(
                            2,
                            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        ),
                    ],
                    &[target(formats[b])],
                    None,
                )
            });
        }
        let white = [255u8; 4];
        Self {
            notes,
            passes,
            terrain,
            water,
            sky,
            entities,
            clouds,
            cloud_draw: None,
            hides_clouds,
            weather,
            weather_draw: None,
            weather_generation: 0,
            hand,
            depth1,
            shadow_targets,
            shadow_terrain,
            shadow_water,
            shadow_comp,
            mipped,
            mips_valid: 0,
            down,
            draws,
            block_ids,
            page_groups: HashMap::new(),
            timing: None,
            custom,
            atlas: world.atlas.as_ref().map(|a| a.0.clone()),
            lightmap: world.lightmap,
            deferred_pre: mask(&post.deferred_pre),
            composite_pre: mask(&post.composite_pre),
            formats,
            clears,
            used,
            blit_depth,
            blit_clears: cleared,
            other_clears,
            blit_group: None,
            main: 0,
            targets: None,
            noise: noise(device, queue),
            dummy: texture(device, queue, "Shader pack stand-in", 1, &white),
            shadow: (
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("Shader pack shadow stand-in"),
                        size: wgpu::Extent3d {
                            width: 1,
                            height: 1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Depth32Float,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default()),
                device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("Shader pack shadow"),
                    compare: Some(wgpu::CompareFunction::LessEqual),
                    ..Default::default()
                }),
            ),
            shadow_cleared: false,
            linear: sampler(
                device,
                wgpu::FilterMode::Linear,
                wgpu::AddressMode::ClampToEdge,
            ),
            nearest: sampler(
                device,
                wgpu::FilterMode::Nearest,
                wgpu::AddressMode::ClampToEdge,
            ),
            repeat: sampler(device, wgpu::FilterMode::Linear, wgpu::AddressMode::Repeat),
            nearest_repeat: sampler(device, wgpu::FilterMode::Nearest, wgpu::AddressMode::Repeat),
            blit,
            copy,
            uniforms: {
                let mut u = FrameUniforms::default();
                u.atlas = world.atlas.map_or([0; 2], |a| a.1);
                u.shadow = shadow_config;
                u
            },
        }
    }

    fn bind(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut Pass,
        out: [u32; 2],
    ) -> wgpu::BindGroup {
        let fog = self.uniforms.fog();
        let ours = [
            [
                out[0] as f32,
                out[1] as f32,
                1.0 / out[0] as f32,
                1.0 / out[1] as f32,
            ],
            fog[0].to_array(),
            fog[1].to_array(),
        ];
        let head: &[u8] = bytemuck::cast_slice(&ours);
        let n = head.len().min(pass.block.len());
        pass.block[..n].copy_from_slice(&head[..n]);
        for (index, ty, offset) in &pass.uniforms {
            if let Some(v) = index.and_then(|i| self.uniforms.values.get(i)) {
                v.write(ty, &mut pass.block, *offset as usize);
            }
        }
        if !pass.block.is_empty() {
            queue.write_buffer(&pass.ubo, 0, &pass.block);
        }
        let targets = self.targets.as_ref().expect("resized");
        let shadow = self.shadow_targets.as_ref();
        let shadow_main = shadow.map_or(0, |s| s.main);
        let shadow_space = pass.group.shadow();
        let (main, current) = if shadow_space {
            (shadow_main, 0)
        } else {
            (self.main, self.mips_valid & self.mipped & pass.reads)
        };
        let key = u64::from(main & pass.reads)
            | u64::from(current) << 16
            | u64::from(shadow_main & pass.shadow_reads) << 32;
        let group = pass.groups.entry(key).or_insert_with(|| {
            let mut entries = vec![wgpu::BindGroupEntry {
                binding: 0,
                resource: pass.ubo.as_entire_binding(),
            }];
            for s in &pass.samplers {
                let shadow_color = |i: usize, side: usize| {
                    shadow.and_then(|t| t.color[i].as_ref()).map(|c| &c[side])
                };
                let texture = match s.source {
                    Source::Color(i) if shadow_space => {
                        shadow_color(i, usize::from(key & 1 << i != 0)).unwrap_or(&self.dummy)
                    }
                    Source::ShadowColor(i) => {
                        shadow_color(i, usize::from(key & 1 << (32 + i) != 0))
                            .unwrap_or(&self.dummy)
                    }
                    Source::ShadowDepth(i, _) => match shadow {
                        Some(t) => match (i, &t.depth1) {
                            (1, Some(d)) => &d.1,
                            _ => &t.depth.1,
                        },
                        None => &self.shadow.0,
                    },
                    Source::Color(i) => {
                        let side = usize::from(key & 1 << i != 0);
                        match (&targets.mips[i], &targets.color[i]) {
                            (Some(m), _) if current & 1 << i != 0 => &m[side].all,
                            (_, Some(t)) => &t[side],
                            (_, None) => &self.dummy,
                        }
                    }
                    Source::Depth => &targets.depth,
                    Source::Depth1 => match &targets.depth1 {
                        Some(d)
                            if matches!(
                                pass.group,
                                Group::Composite | Group::Final | Group::Translucent
                            ) =>
                        {
                            &d.1
                        }
                        _ => &targets.depth,
                    },
                    Source::Noise => &self.noise,
                    Source::Dummy => &self.dummy,
                    Source::Shadow => &self.shadow.0,
                    Source::Atlas => pass
                        .atlas
                        .as_ref()
                        .or(self.atlas.as_ref())
                        .unwrap_or(&self.dummy),
                    Source::Lightmap => &self.lightmap,
                };
                let sampler = match SamplerKind::of(s.source, s.filtering) {
                    SamplerKind::Linear => &self.linear,
                    SamplerKind::Nearest => &self.nearest,
                    SamplerKind::Repeat => &self.repeat,
                    SamplerKind::NearestRepeat => &self.nearest_repeat,
                    SamplerKind::Comparison => shadow.map_or(&self.shadow.1, |t| &t.comparison),
                };
                entries.push(wgpu::BindGroupEntry {
                    binding: s.binding,
                    resource: wgpu::BindingResource::TextureView(texture),
                });
                if s.sampler == s.binding + 1 {
                    entries.push(wgpu::BindGroupEntry {
                        binding: s.sampler,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    });
                }
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&pass.name),
                layout: &pass.layout,
                entries: &entries,
            })
        });
        group.clone()
    }

    fn begin<'e>(
        &mut self,
        encoder: &'e mut wgpu::CommandEncoder,
        pass: &Pass,
    ) -> wgpu::RenderPass<'e> {
        let targets = self.targets.as_ref().expect("resized");
        let load = wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
        };
        let (depth_ops, shadow_ops) = match &mut self.shadow_targets {
            Some(s) if pass.group.shadow() => s.clear_ops(pass.writes.iter().map(|w| w.1)),
            _ => (super::pack_shadow::load(), [load; 2]),
        };
        let shadow = self.shadow_targets.as_ref().filter(|_| pass.group.shadow());
        let mut attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = Vec::new();
        for &(location, buffer) in &pass.writes {
            let l = location as usize;
            if attachments.len() <= l {
                attachments.resize(l + 1, None);
            }
            attachments[l] = match shadow {
                Some(s) => s
                    .color(buffer, false)
                    .and_then(|v| attach(v, shadow_ops.get(buffer).copied().unwrap_or(load))),
                None => targets.color[buffer]
                    .as_ref()
                    .and_then(|t| attach(&t[usize::from(self.main & 1 << buffer != 0)], load)),
            };
        }
        let timestamp_writes = self.timing.as_mut().and_then(|t| t.writes(&pass.name));
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&pass.name),
            color_attachments: &attachments,
            timestamp_writes,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: shadow.map_or(&targets.depth, |s| &s.depth.1),
                depth_ops: Some(depth_ops),
                stencil_ops: None,
            }),
            ..Default::default()
        })
    }

    fn draw_terrain(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        list: TerrainList,
        draw: &mut TerrainDraw<'_>,
    ) {
        let slot = match list {
            TerrainList::Solid => &mut self.terrain,
            TerrainList::Translucent => &mut self.water,
            TerrainList::ShadowSolid => &mut self.shadow_terrain,
            TerrainList::ShadowTranslucent => &mut self.shadow_water,
        };
        let Some(mut pass) = slot.take() else {
            return;
        };
        let translucent = list.translucent();
        if list == TerrainList::Translucent
            && let Some(t) = &self.targets
            && let Some((copy, _)) = &t.depth1
        {
            encoder.copy_texture_to_texture(
                t.depth_texture.as_image_copy(),
                copy.as_image_copy(),
                t.depth_texture.size(),
            );
        }
        let group = self.bind(device, queue, &mut pass, size);
        {
            let mut rp = self.begin(encoder, &pass);
            rp.set_pipeline(&pass.pipeline);
            rp.set_bind_group(0, &group, &[]);
            if pass.draw {
                let stage = if translucent {
                    "TERRAIN_TRANSLUCENT"
                } else {
                    "TERRAIN_SOLID"
                };
                self.draws.bind(&mut rp, stage);
            }
            let (groups, ids) = (&self.page_groups, pass.ids);
            let page = |rp: &mut wgpu::RenderPass<'_>, page: u32| {
                if let Some((_, group)) = groups.get(&page).filter(|_| ids) {
                    rp.set_bind_group(3, group, &[]);
                }
            };
            draw(&mut rp, list, &page);
        }
        if !pass.group.shadow() {
            self.mips_valid &= !pass.written;
        }
        *match list {
            TerrainList::Solid => &mut self.terrain,
            TerrainList::Translucent => &mut self.water,
            TerrainList::ShadowSolid => &mut self.shadow_terrain,
            TerrainList::ShadowTranslucent => &mut self.shadow_water,
        } = Some(pass);
    }

    fn draw_sky(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        frame: &super::Frame,
    ) {
        let Some(mut sky) = self.sky.take() else {
            return;
        };
        let parts = sky
            .geometry
            .prepare(queue, frame, self.uniforms.shadow.sun_path_rotation);
        let mut groups = [None, None];
        for (i, pass) in [&mut sky.basic, &mut sky.textured].into_iter().enumerate() {
            if let Some(pass) = pass {
                groups[i] = Some(self.bind(device, queue, pass, size));
                self.mips_valid &= !pass.written;
            }
        }
        let passes = [&sky.basic, &sky.textured];
        let mut open: Option<(wgpu::RenderPass<'_>, usize)> = None;
        for part in parts {
            let i = usize::from(part.textured);
            let (Some(pass), Some(group)) = (passes[i], &groups[i]) else {
                continue;
            };
            let current = open.as_ref().map(|o| o.1);
            if current != Some(i) {
                let same = current
                    .and_then(|c| passes[c].as_ref())
                    .is_some_and(|p| p.writes == pass.writes);
                if !same {
                    drop(open.take());
                    open = Some((self.begin(encoder, pass).forget_lifetime(), i));
                }
                let rp = &mut open.as_mut().expect("open").0;
                rp.set_pipeline(&pass.pipeline);
                rp.set_bind_group(0, group, &[]);
                sky.geometry.bind(rp, part.textured);
                open.as_mut().expect("open").1 = i;
            }
            let rp = &mut open.as_mut().expect("open").0;
            if pass.draw {
                self.draws.bind(rp, part.stage);
            }
            rp.draw(part.range, 0..1);
        }
        drop(open);
        self.sky = Some(sky);
    }

    fn draw_entities(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        late: bool,
    ) {
        let Some(mut entities) = self.entities.take() else {
            return;
        };
        let geometry = &entities.geometry;
        for (program, pass) in entities.passes.iter_mut().enumerate() {
            let Some(pass) = pass.as_mut().filter(|_| entities.late[program] == late) else {
                continue;
            };
            let mut draws = geometry
                .draws
                .iter()
                .filter(|d| d.pack && d.program == program)
                .peekable();
            if draws.peek().is_none() {
                continue;
            }
            let group = self.bind(device, queue, pass, size);
            {
                let mut rp = self.begin(encoder, pass);
                rp.set_pipeline(&pass.pipeline);
                rp.set_bind_group(0, &group, &[]);
                rp.set_bind_group(1, &geometry.table_group, &[]);
                rp.set_vertex_buffer(0, geometry.vertices());
                rp.set_index_buffer(geometry.indices.slice(..), wgpu::IndexFormat::Uint32);
                for d in draws {
                    if pass.draw {
                        rp.set_bind_group(2, &geometry.group, &[geometry.offset(d.slot)]);
                    }
                    rp.draw_indexed(0..d.quads * 6, d.base as i32, 0..1);
                }
            }
            self.mips_valid &= !pass.written;
        }
        self.entities = Some(entities);
    }

    fn draw_shadow_entities(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
    ) {
        let Some(mut entities) = self.entities.take() else {
            return;
        };
        let geometry = &entities.geometry;
        for (program, pass) in entities.shadow.iter_mut().enumerate() {
            let Some(pass) = pass else {
                continue;
            };
            let mut draws = (geometry.draws.iter())
                .filter(|d| d.shadow && d.program == program)
                .peekable();
            if draws.peek().is_none() {
                continue;
            }
            let group = self.bind(device, queue, pass, size);
            let mut rp = self.begin(encoder, pass);
            rp.set_pipeline(&pass.pipeline);
            rp.set_bind_group(0, &group, &[]);
            rp.set_bind_group(1, &geometry.table_group, &[]);
            rp.set_vertex_buffer(0, geometry.vertices());
            rp.set_index_buffer(geometry.indices.slice(..), wgpu::IndexFormat::Uint32);
            for d in draws {
                if pass.draw {
                    rp.set_bind_group(2, &geometry.group, &[geometry.offset(d.slot)]);
                }
                rp.draw_indexed(0..d.quads * 6, d.base as i32, 0..1);
            }
        }
        self.entities = Some(entities);
    }

    pub fn prepare_entities(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        entities: &[super::Vertex],
        spans: &[super::pack_draw::EntitySpan],
        rest: &mut Vec<super::Vertex>,
    ) {
        let shadow = self.shadow_casters();
        let Some(e) = &mut self.entities else {
            rest.clear();
            rest.extend_from_slice(entities);
            return;
        };
        let drawn = e.passes.each_ref().map(Option::is_some);
        e.geometry.prepare(
            device,
            queue,
            &self.draws.layout,
            entities,
            spans,
            drawn,
            shadow,
            rest,
        );
    }

    pub fn shadow_casters(&self) -> super::pack_entities::Casters {
        let Some(e) = self
            .entities
            .as_ref()
            .filter(|_| self.shadow_targets.is_some())
        else {
            return Default::default();
        };
        super::pack_entities::Casters {
            entities: e.shadow[0].is_some(),
            only_player: e.only.0,
            blocks: e.shadow[1].is_some(),
            only_lit: e.only.1,
        }
    }

    pub fn has_entities(&self) -> bool {
        self.entities.is_some()
    }

    fn draw_opaque(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        frame: &super::Frame,
        terrain: &mut TerrainDraw<'_>,
    ) {
        if super::sky_visible(frame) {
            self.draw_sky(device, queue, encoder, size, frame);
        }
        self.draw_terrain(device, queue, encoder, size, TerrainList::Solid, terrain);
        self.draw_entities(device, queue, encoder, size, false);
        self.draw_clouds(device, queue, encoder, size);
        self.draw_hand(device, queue, encoder, size, frame);
    }

    fn draw_hand(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        frame: &super::Frame,
    ) {
        let Some((mut pass, mut geometry)) = self.hand.take() else {
            return;
        };
        let projection = super::pack_hand::projection(
            frame.hand_fov_degrees,
            size[0] as f32 / size[1].max(1) as f32,
            frame.depth_far,
            frame.bob,
        );
        geometry.prepare(device, queue, frame.hand, projection);
        if geometry.quads > 0 {
            let group = self.bind(device, queue, &mut pass, size);
            {
                let mut rp = self.begin(encoder, &pass);
                rp.set_pipeline(&pass.pipeline);
                rp.set_bind_group(0, &group, &[]);
                rp.set_bind_group(1, &geometry.group, &[]);
                if pass.draw {
                    self.draws.bind(&mut rp, "HAND_SOLID");
                }
                rp.set_vertex_buffer(0, geometry.vertices());
                rp.set_index_buffer(geometry.indices.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..geometry.quads * 6, 0, 0..1);
            }
            self.mips_valid &= !pass.written;
        }
        self.hand = Some((pass, geometry));
    }

    pub fn has_hand(&self) -> bool {
        self.hand.is_some()
    }

    fn draw_clouds(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
    ) {
        let (Some(mut pass), Some(clouds)) = (self.clouds.take(), self.cloud_draw.take()) else {
            return;
        };
        let group = self.bind(device, queue, &mut pass, size);
        {
            let mut rp = self.begin(encoder, &pass);
            rp.set_pipeline(&pass.pipeline);
            rp.set_bind_group(0, &group, &[]);
            rp.set_bind_group(1, &clouds.group, &[]);
            if pass.draw {
                self.draws.bind(&mut rp, "CLOUDS");
            }
            rp.set_vertex_buffer(0, clouds.vertices.slice(..));
            rp.set_index_buffer(clouds.indices.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..clouds.quads * 6, 0, 0..1);
        }
        self.mips_valid &= !pass.written;
        self.clouds = Some(pass);
    }

    pub fn has_clouds(&self) -> bool {
        self.hides_clouds
    }

    pub fn set_clouds(&mut self, clouds: Option<Clouds>) {
        self.cloud_draw = clouds.filter(|c| c.quads > 0);
    }

    fn draw_translucent(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
        terrain: &mut TerrainDraw<'_>,
    ) {
        self.draw_terrain(
            device,
            queue,
            encoder,
            size,
            TerrainList::Translucent,
            terrain,
        );
        self.draw_entities(device, queue, encoder, size, true);
        self.draw_weather(device, queue, encoder, size);
    }

    fn draw_weather(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        size: [u32; 2],
    ) {
        let (Some(mut passes), Some(weather)) = (self.weather.take(), self.weather_draw.take())
        else {
            return;
        };
        if weather.generation != self.weather_generation {
            self.weather_generation = weather.generation;
            for (pass, sprite) in passes.iter_mut().zip(&weather.sprites) {
                pass.atlas = sprite.clone();
                pass.groups.clear();
            }
        }
        for (pass, quads) in passes.iter_mut().zip(weather.quads.clone()) {
            if quads.is_empty() {
                continue;
            }
            let group = self.bind(device, queue, pass, size);
            {
                let mut rp = self.begin(encoder, pass);
                rp.set_pipeline(&pass.pipeline);
                rp.set_bind_group(0, &group, &[]);
                if pass.draw {
                    self.draws.bind(&mut rp, "RAIN_SNOW");
                }
                rp.set_vertex_buffer(0, weather.vertices.slice(..));
                rp.set_index_buffer(weather.indices.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(quads.start * 6..quads.end * 6, 0, 0..1);
            }
            self.mips_valid &= !pass.written;
        }
        self.weather = Some(passes);
    }

    pub fn has_weather(&self) -> bool {
        self.weather.is_some()
    }

    pub fn set_weather(&mut self, weather: Option<Weather>) {
        self.weather_draw = weather;
    }

    fn mipmap(&self, encoder: &mut wgpu::CommandEncoder, b: usize) {
        let (Some(targets), Some((pipeline, _))) = (&self.targets, self.down.get(&self.formats[b]))
        else {
            return;
        };
        let Some(mips) = &targets.mips[b] else { return };
        let side = &mips[usize::from(self.main & 1 << b != 0)];
        for (attachment, group) in &side.levels {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shader pack mipmap"),
                color_attachments: &[attach(
                    attachment,
                    wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                )],
                ..Default::default()
            });
            rp.set_pipeline(pipeline);
            rp.set_bind_group(0, group, &[]);
            rp.draw(0..3, 0..1);
        }
    }

    pub fn shadow_view(&self, frame: &super::Frame) -> Option<ShadowView> {
        self.shadow_targets.as_ref()?;
        if self.shadow_terrain.is_none() && self.shadow_water.is_none() {
            return None;
        }
        let shadow = &self.uniforms.shadow;
        Some(ShadowView {
            cull: pack_uniforms::shadow_culling(frame, shadow),
            light: pack_uniforms::shadow_light(frame, shadow),
            distance: (shadow.render_mul > 0.0).then_some(shadow.distance * shadow.render_mul),
        })
    }

    pub fn has_terrain(&self) -> bool {
        self.terrain.is_some()
    }

    pub fn has_sky(&self) -> bool {
        self.sky.as_ref().is_some_and(|s| s.basic.is_some())
    }

    pub fn has_celestial(&self) -> bool {
        self.sky.as_ref().is_some_and(|s| s.textured.is_some())
    }

    pub fn has_water(&self) -> bool {
        self.water.is_some()
    }

    pub fn programs(&self) -> Vec<&str> {
        self.passes.iter().map(|p| p.name.as_str()).collect()
    }

    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let full_chain = 32 - size[0].max(size[1]).max(1).leading_zeros();
        let texture_levels = |label: &str, format, usage, levels| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: usage | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let texture = |label: &str, format, usage| texture_levels(label, format, usage, 1);
        let level = |t: &wgpu::Texture, level: u32| {
            t.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let mut mips: [Option<[Mips; 2]>; BUFFERS] = Default::default();
        let color = std::array::from_fn(|i| {
            (self.used & 1 << i != 0).then(|| {
                let label = format!("colortex{i}");
                let mipped = self.mipped & 1 << i != 0;
                let levels = if mipped { full_chain } else { 1 };
                let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
                let sides =
                    [0, 1].map(|_| texture_levels(&label, self.formats[i], attachment, levels));
                if mipped && let Some((_, layout)) = self.down.get(&self.formats[i]) {
                    mips[i] = Some(sides.each_ref().map(|t| {
                        Mips {
                            all: t.create_view(&Default::default()),
                            levels: (1..levels)
                                .map(|l| {
                                    let group =
                                        device.create_bind_group(&wgpu::BindGroupDescriptor {
                                            label: Some(&label),
                                            layout,
                                            entries: &[
                                                wgpu::BindGroupEntry {
                                                    binding: 0,
                                                    resource: wgpu::BindingResource::TextureView(
                                                        &level(t, l - 1),
                                                    ),
                                                },
                                                wgpu::BindGroupEntry {
                                                    binding: 2,
                                                    resource: wgpu::BindingResource::Sampler(
                                                        &self.linear,
                                                    ),
                                                },
                                            ],
                                        });
                                    (level(t, l), group)
                                })
                                .collect(),
                        }
                    }));
                }
                sides.map(|t| level(&t, 0))
            })
        });
        let depth32 = wgpu::TextureFormat::Depth32Float;
        let depth_texture = texture(
            "depthtex0",
            depth32,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth1 = self.depth1.then(|| {
            let t = texture("depthtex1", depth32, wgpu::TextureUsages::COPY_DST);
            let view = t.create_view(&Default::default());
            (t, view)
        });
        self.targets = Some(Targets {
            size,
            color,
            depth: depth_texture.create_view(&Default::default()),
            depth_texture,
            depth1,
            mips,
        });
        let gbuffers = self.terrain.iter_mut().chain(&mut self.water);
        for pass in self.passes.iter_mut().chain(gbuffers) {
            pass.groups.clear();
        }
    }

    fn run_shadow(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        terrain: &mut TerrainDraw<'_>,
    ) {
        let Some(t) = &mut self.shadow_targets else {
            return;
        };
        t.begin_frame();
        let size = [t.size; 2];
        self.draw_terrain(
            device,
            queue,
            encoder,
            size,
            TerrainList::ShadowSolid,
            terrain,
        );
        self.draw_shadow_entities(device, queue, encoder, size);
        let t = self.shadow_targets.as_mut().expect("shadow pass");
        t.clear_rest(encoder);
        t.copy_depth1(encoder);
        self.draw_terrain(
            device,
            queue,
            encoder,
            size,
            TerrainList::ShadowTranslucent,
            terrain,
        );
        let mut comp = std::mem::take(&mut self.shadow_comp);
        for pass in &mut comp {
            let group = self.bind(device, queue, pass, size);
            let t = self.shadow_targets.as_ref().expect("shadow pass");
            let mut attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = Vec::new();
            for &(location, buffer) in &pass.writes {
                let l = location as usize;
                if attachments.len() <= l {
                    attachments.resize(l + 1, None);
                }
                let ops = wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                };
                attachments[l] = t.color(buffer, true).and_then(|v| attach(v, ops));
            }
            {
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&pass.name),
                    color_attachments: &attachments,
                    timestamp_writes: self.timing.as_mut().and_then(|t| t.writes(&pass.name)),
                    ..Default::default()
                });
                rp.set_pipeline(&pass.pipeline);
                rp.set_bind_group(0, &group, &[]);
                rp.draw(0..6, 0..1);
            }
            if let Some(t) = &mut self.shadow_targets {
                t.main ^= pass.flip;
            }
        }
        self.shadow_comp = comp;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        world: &wgpu::TextureView,
        world_depth: &wgpu::TextureView,
        screen: &wgpu::TextureView,
        frame: &super::Frame,
        inputs: &PackInputs,
        size: [u32; 2],
        window: [u32; 2],
        pages: &[(u32, wgpu::Buffer)],
        terrain: &mut TerrainDraw<'_>,
    ) {
        self.resize(device, size);
        if let Some((layout, ids)) = &self.block_ids {
            self.page_groups
                .retain(|p, (b, _)| pages.iter().any(|(q, c)| q == p && c == b));
            for (page, buffer) in pages {
                self.page_groups.entry(*page).or_insert_with(|| {
                    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Shader pack terrain"),
                        layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: ids.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: buffer.as_entire_binding(),
                            },
                        ],
                    });
                    (buffer.clone(), group)
                });
            }
        }
        if let Some(t) = &mut self.timing {
            t.begin(device, queue);
        }
        self.uniforms.update(frame, inputs, size);
        self.uniforms.eval_custom(&mut self.custom);
        if self.targets.is_none() {
            return;
        }
        self.run_shadow(device, queue, encoder, terrain);
        let Some(targets) = &self.targets else { return };
        let main = |m: u16, i: usize| usize::from(m & 1 << i != 0);
        let view = |m: u16, i: usize, alt: bool| {
            targets.color[i]
                .as_ref()
                .map(|t| &t[main(m, i) ^ usize::from(alt)])
        };

        if !self.shadow_cleared {
            self.shadow_cleared = true;
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shader pack shadow stand-in"),
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow.0,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
        }
        let clear_op = |b: usize| {
            let c = self.clears[b].unwrap_or_default();
            wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: c[0].into(),
                    g: c[1].into(),
                    b: c[2].into(),
                    a: c[3].into(),
                }),
                store: wgpu::StoreOp::Store,
            }
        };
        let overwrite = wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        };
        for chunk in self.other_clears.chunks(8) {
            let attachments: Vec<_> = chunk
                .iter()
                .map(|&b| attach(view(self.main, b, false).expect("used buffer"), clear_op(b)))
                .collect();
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shader pack clear"),
                color_attachments: &attachments,
                timestamp_writes: self.timing.as_mut().and_then(|t| t.writes("clear")),
                ..Default::default()
            });
        }

        if self.blit_group.as_ref().is_none_or(|g| g.0 != size) {
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Shader pack world"),
                layout: &self.blit.1,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(world),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(world_depth),
                    },
                ],
            });
            self.blit_group = Some((size, group));
        }
        {
            let mut attachments = vec![attach(
                view(self.main, 0, false).expect("colortex0"),
                overwrite,
            )];
            for &b in &self.blit_clears {
                attachments.push(attach(
                    view(self.main, b, false).expect("used buffer"),
                    clear_op(b),
                ));
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shader pack world"),
                color_attachments: &attachments,
                timestamp_writes: self.timing.as_mut().and_then(|t| t.writes("world")),
                depth_stencil_attachment: self.blit_depth.then_some(
                    wgpu::RenderPassDepthStencilAttachment {
                        view: &targets.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    },
                ),
                ..Default::default()
            });
            pass.set_pipeline(&self.blit.0);
            pass.set_bind_group(0, self.blit_group.as_ref().map(|g| &g.1), &[]);
            pass.draw(0..3, 0..1);
        }

        self.mips_valid = 0;
        let mut passes = std::mem::take(&mut self.passes);
        let mut seen = (false, false);
        let mut shown = false;
        let mut terrain_drawn = self.terrain.is_none()
            && self.sky.is_none()
            && !self.entities.as_ref().is_some_and(Entities::drawn)
            && self.clouds.is_none()
            && self.hand.is_none();
        let late_entities = self.entities.as_ref().is_some_and(|e| {
            e.passes
                .iter()
                .zip(e.late)
                .any(|(p, late)| p.is_some() && late)
        });
        let mut water_drawn = self.water.is_none() && self.weather.is_none() && !late_entities;
        for pass in &mut passes {
            if !terrain_drawn && pass.group != Group::Prepare {
                terrain_drawn = true;
                self.draw_opaque(device, queue, encoder, size, frame, terrain);
            }
            if !water_drawn && matches!(pass.group, Group::Composite | Group::Final) {
                water_drawn = true;
                self.draw_translucent(device, queue, encoder, size, terrain);
            }
            if pass.group == Group::Deferred && !seen.0 {
                seen.0 = true;
                self.main ^= self.deferred_pre;
            }
            if pass.group == Group::Composite && !seen.1 {
                seen.1 = true;
                self.main ^= self.composite_pre;
            }
            let stale = pass.mipmaps & self.mipped & !self.mips_valid;
            for b in (0..BUFFERS).filter(|b| stale & 1 << b != 0) {
                self.mipmap(encoder, b);
            }
            self.mips_valid |= stale;
            let final_pass = pass.group == Group::Final;
            let group = self.bind(device, queue, pass, if final_pass { window } else { size });
            let targets = self.targets.as_ref().expect("resized");
            let view = |i: usize| {
                targets.color[i]
                    .as_ref()
                    .map(|t| &t[usize::from(self.main & 1 << i == 0)])
            };
            let mut attachments: Vec<Option<wgpu::RenderPassColorAttachment>> = Vec::new();
            if final_pass {
                attachments.push(attach(screen, overwrite));
                shown = true;
            } else {
                for &(location, buffer) in &pass.writes {
                    let l = location as usize;
                    if attachments.len() <= l {
                        attachments.resize(l + 1, None);
                    }
                    attachments[l] = view(buffer).and_then(|v| attach(v, overwrite));
                }
            }
            {
                let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(&pass.name),
                    color_attachments: &attachments,
                    timestamp_writes: self.timing.as_mut().and_then(|t| t.writes(&pass.name)),
                    ..Default::default()
                });
                rp.set_pipeline(&pass.pipeline);
                rp.set_bind_group(0, &group, &[]);
                rp.draw(0..6, 0..1);
            }
            self.main ^= pass.flip;
            self.mips_valid &= !pass.written;
        }
        self.passes = passes;
        if !terrain_drawn {
            self.draw_opaque(device, queue, encoder, size, frame, terrain);
        }
        if !water_drawn {
            self.draw_translucent(device, queue, encoder, size, terrain);
        }
        let targets = self.targets.as_ref().expect("resized");
        let view = |m: u16, i: usize, alt: bool| {
            targets.color[i]
                .as_ref()
                .map(|t| &t[main(m, i) ^ usize::from(alt)])
        };
        if !shown {
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Shader pack colortex0"),
                layout: &self.copy.1,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            view(self.main, 0, false).expect("colortex0"),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.linear),
                    },
                ],
            });
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shader pack colortex0"),
                color_attachments: &[attach(screen, overwrite)],
                ..Default::default()
            });
            rp.set_pipeline(&self.copy.0);
            rp.set_bind_group(0, &group, &[]);
            rp.draw(0..3, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::TextureFormat as T;

    #[test]
    fn samplers_of_a_kind_share_one_variable() {
        let mut module = naga::front::wgsl::parse_str(
            "@group(0) @binding(1) var a: texture_2d<f32>;\n\
             @group(0) @binding(2) var sa: sampler;\n\
             @group(0) @binding(3) var b: texture_2d<f32>;\n\
             @group(0) @binding(4) var sb: sampler;\n\
             @fragment fn main() -> @location(0) vec4<f32> {\n\
             return textureSample(a, sa, vec2(0.5)) + textureSample(b, sb, vec2(0.5));\n}\n",
        )
        .unwrap();
        share_samplers(&mut module, &HashMap::from([(2, 2), (4, 2)]));
        let info = naga::valid::Validator::new(Default::default(), Default::default())
            .validate(&module)
            .unwrap();
        let used: Vec<u32> = module
            .global_variables
            .iter()
            .filter(|(h, _)| !info.get_entry_point(0)[*h].is_empty())
            .filter_map(|(_, g)| g.binding.as_ref().map(|b| b.binding))
            .collect();
        assert_eq!(used, [1, 2, 3]);
    }

    #[test]
    fn a_shader_compiler_panic_skips_the_pass() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
            return;
        };
        let Ok((device, _queue)) = pollster::block_on(adapter.request_device(&Default::default()))
        else {
            return;
        };
        let failed = checked(&device, || -> wgpu::Buffer {
            panic!("internal error: entered unreachable code: Expression [89] is not cached!")
        });
        let error = failed.expect_err("the panic comes back as an error");
        assert!(error.contains("Expression [89] is not cached!"), "{error}");
        let buffer = |usage| {
            checked(&device, || {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 16,
                    usage,
                    mapped_at_creation: false,
                })
            })
        };
        assert!(buffer(wgpu::BufferUsages::UNIFORM).is_ok());
        assert!(buffer(wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::MAP_WRITE).is_err());
    }

    #[test]
    fn pack_formats_map_to_renderable_ones() {
        let f = |name| texture_format(Format::parse(name).unwrap(), wgpu::Features::empty());
        assert_eq!(f("RGBA"), T::Rgba8Unorm);
        assert_eq!(f("RGB8"), T::Rgba8Unorm, "no three-channel formats in wgpu");
        assert_eq!(f("RG16F"), T::Rg16Float);
        assert_eq!(f("R32F"), T::R32Float);
        assert_eq!(f("RGB10_A2"), T::Rgb10a2Unorm);
        assert_eq!(f("R32UI"), T::R32Uint);
        assert_eq!(f("RGBA16"), T::Rgba16Float, "16-bit norm needs a feature");
        assert_eq!(f("RGBA8_SNORM"), T::Rgba16Float, "snorm is not renderable");
        assert_eq!(f("R11F_G11F_B10F"), T::Rgba16Float);
        let all = wgpu::Features::TEXTURE_FORMAT_16BIT_NORM
            | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
            | wgpu::Features::RG11B10UFLOAT_RENDERABLE;
        let g = |name| texture_format(Format::parse(name).unwrap(), all);
        assert_eq!(g("RGBA16"), T::Rgba16Unorm);
        assert_eq!(g("R11F_G11F_B10F"), T::Rg11b10Ufloat);
    }

    #[test]
    fn sampler_names() {
        let main = |n| source(n, false, false, false);
        assert!(main("gaux1") == Source::Color(4));
        assert!(main("gdepthtex") == Source::Depth);
        assert!(main("depthtex2") == Source::Depth1);
        assert!(main("noisetex") == Source::Noise);
        assert!(main("shadowcolor0") == Source::Dummy);
        assert!(main("shadowtex0") == Source::Dummy);
        assert!(source("shadowcolor", false, true, false) == Source::ShadowColor(0));
        assert!(source("shadowcolor1", true, true, false) == Source::Color(1));
        assert!(source("colortex1", true, true, false) == Source::Dummy);
        assert!(source("shadow", false, true, false) == Source::ShadowDepth(0, false));
        assert!(source("shadow", false, true, true) == Source::ShadowDepth(1, false));
        assert!(source("watershadow", false, true, true) == Source::ShadowDepth(0, false));
        assert!(source("shadowtex1HW", false, true, false) == Source::ShadowDepth(1, true));
    }
}
