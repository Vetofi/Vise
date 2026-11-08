use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};

pub const FILE: &str = "vise-graphics.txt";
pub const HDR_MARKER: &str = "vise-hdr-pending";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reported {
    pub paper_white: u32,
    pub peak: u32,
}

static REPORTED: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];

pub fn report(reported: Option<Reported>) {
    let (paper_white, peak) = reported.map_or((0, 0), |r| (r.paper_white, r.peak));
    REPORTED[0].store(paper_white, Ordering::Relaxed);
    REPORTED[1].store(peak, Ordering::Relaxed);
}
pub fn reported() -> Option<Reported> {
    let [paper_white, peak] = REPORTED.each_ref().map(|n| n.load(Ordering::Relaxed));
    (peak > 0).then_some(Reported { paper_white, peak })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Preset {
    Performance,
    Balanced,
    Quality,
    Ultra,
    #[default]
    Custom,
}
impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::Performance,
        Preset::Balanced,
        Preset::Quality,
        Preset::Ultra,
        Preset::Custom,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Preset::Performance => "performance",
            Preset::Balanced => "balanced",
            Preset::Quality => "quality",
            Preset::Ultra => "ultra",
            Preset::Custom => "custom",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Preset::Performance => "Performance",
            Preset::Balanced => "Balanced",
            Preset::Quality => "Quality",
            Preset::Ultra => "Ultra",
            Preset::Custom => "Custom",
        }
    }
    fn parse(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|p| p.name() == s)
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PresentMode {
    #[default]
    Auto,
    Fifo,
    Mailbox,
    Immediate,
}
impl PresentMode {
    pub const ALL: [PresentMode; 4] = [
        PresentMode::Auto,
        PresentMode::Fifo,
        PresentMode::Mailbox,
        PresentMode::Immediate,
    ];
    pub fn name(self) -> &'static str {
        match self {
            PresentMode::Auto => "auto",
            PresentMode::Fifo => "fifo",
            PresentMode::Mailbox => "mailbox",
            PresentMode::Immediate => "immediate",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            PresentMode::Auto => "Auto (VSync)",
            PresentMode::Fifo => "FIFO",
            PresentMode::Mailbox => "Mailbox",
            PresentMode::Immediate => "Immediate",
        }
    }
    fn parse(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|p| p.name() == s)
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GraphicsApi {
    #[default]
    Auto,
    Vulkan,
    Dx12,
    OpenGl,
}
impl GraphicsApi {
    pub const ALL: [GraphicsApi; 4] = [
        GraphicsApi::Auto,
        GraphicsApi::Vulkan,
        GraphicsApi::Dx12,
        GraphicsApi::OpenGl,
    ];
    pub fn choices() -> &'static [GraphicsApi] {
        const WINDOWS: [GraphicsApi; 4] = [
            GraphicsApi::Auto,
            GraphicsApi::Vulkan,
            GraphicsApi::Dx12,
            GraphicsApi::OpenGl,
        ];
        const LINUX: [GraphicsApi; 3] =
            [GraphicsApi::Auto, GraphicsApi::Vulkan, GraphicsApi::OpenGl];
        const MACOS: [GraphicsApi; 1] = [GraphicsApi::Auto];
        if cfg!(windows) {
            &WINDOWS
        } else if cfg!(target_os = "macos") {
            &MACOS
        } else {
            &LINUX
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            GraphicsApi::Auto => "auto",
            GraphicsApi::Vulkan => "vulkan",
            GraphicsApi::Dx12 => "dx12",
            GraphicsApi::OpenGl => "opengl",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            GraphicsApi::Auto => "Auto",
            GraphicsApi::Vulkan => "Vulkan",
            GraphicsApi::Dx12 => "DirectX 12",
            GraphicsApi::OpenGl => "OpenGL",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|api| api.name().eq_ignore_ascii_case(s.trim()))
    }
    pub fn with_env(self) -> Self {
        std::env::var("VISE_GRAPHICS_API")
            .ok()
            .and_then(|v| Self::from_name(&v))
            .unwrap_or(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pacing {
    #[default]
    Efficient,
    Precise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    Off,
    Fps,
    Graph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FarDetail {
    #[default]
    Full,
    Reduced,
}
impl FarDetail {
    pub fn name(self) -> &'static str {
        match self {
            FarDetail::Full => "full",
            FarDetail::Reduced => "reduced",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            FarDetail::Full => "Full",
            FarDetail::Reduced => "Reduced",
        }
    }
    fn parse(s: &str) -> Self {
        if s == "reduced" {
            FarDetail::Reduced
        } else {
            FarDetail::Full
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Upscaling {
    #[default]
    Native,
    Quality,
    Balanced,
    Performance,
    UltraPerformance,
}
impl Upscaling {
    pub const ALL: [Upscaling; 5] = [
        Upscaling::Native,
        Upscaling::Quality,
        Upscaling::Balanced,
        Upscaling::Performance,
        Upscaling::UltraPerformance,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Upscaling::Native => "native",
            Upscaling::Quality => "quality",
            Upscaling::Balanced => "balanced",
            Upscaling::Performance => "performance",
            Upscaling::UltraPerformance => "ultraPerformance",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Upscaling::Native => "Native",
            Upscaling::Quality => "Quality",
            Upscaling::Balanced => "Balanced",
            Upscaling::Performance => "Performance",
            Upscaling::UltraPerformance => "Ultra Performance",
        }
    }
    pub fn ratio(self) -> f32 {
        match self {
            Upscaling::Native => 1.0,
            Upscaling::Quality => 1.5,
            Upscaling::Balanced => 1.7,
            Upscaling::Performance => 2.0,
            Upscaling::UltraPerformance => 3.0,
        }
    }
    fn parse(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|u| u.name() == s)
            .unwrap_or_default()
    }
}

static REDUCED_SUGGESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn suggest_reduced_far_detail(on: bool) {
    REDUCED_SUGGESTED.store(on, std::sync::atomic::Ordering::Relaxed);
}
pub fn reduced_far_detail_suggested() -> bool {
    REDUCED_SUGGESTED.load(std::sync::atomic::Ordering::Relaxed)
}

static HDR_OFFERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn offer_hdr(on: bool) {
    HDR_OFFERED.store(on, std::sync::atomic::Ordering::Relaxed);
}
pub fn hdr_offered() -> bool {
    HDR_OFFERED.load(std::sync::atomic::Ordering::Relaxed)
}

pub const INTEGRATED_LOD_DISTANCE: u32 = 128;
pub fn lod_capped(info: &wgpu::AdapterInfo) -> bool {
    info.device_type == wgpu::DeviceType::IntegratedGpu && !info.name.starts_with("Apple")
}
pub fn lod_distance_capped(distance: u32, info: &wgpu::AdapterInfo) -> u32 {
    let distance = distance.min(1024);
    if lod_capped(info) {
        distance.min(INTEGRATED_LOD_DISTANCE)
    } else {
        distance
    }
}
impl Graphics {
    pub fn lod_distance_on(&self, info: &wgpu::AdapterInfo) -> u32 {
        lod_distance_capped(self.lod_distance, info)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Graphics {
    pub preset: Preset,
    pub lod_distance: u32,
    pub far_detail: FarDetail,
    pub cave_culling: bool,
    pub entity_culling: bool,
    pub chunk_updates: u32,
    pub upload_budget_kib: u32,
    pub sky: bool,
    pub fog: bool,
    pub weather: bool,
    pub animated_textures: bool,
    pub present_mode: PresentMode,
    pub pacing: Pacing,
    pub workers: u32,
    pub overlay: Overlay,
    pub resolution_scale: u32,
    pub fxaa: bool,
    pub taa: bool,
    pub upscaling: Upscaling,
    pub hdr: bool,
    pub hdr_paper_white: u32,
    pub hdr_peak: u32,
    pub hdr_black_level: u32,
    pub hdr_ui_brightness: u32,
    pub battery: bool,
    pub api: GraphicsApi,
    pub shader_pack: String,
}
impl Default for Graphics {
    fn default() -> Self {
        Self {
            preset: Preset::Custom,
            lod_distance: 0,
            far_detail: FarDetail::Full,
            cave_culling: true,
            entity_culling: false,
            chunk_updates: 0,
            upload_budget_kib: 0,
            sky: true,
            fog: true,
            weather: true,
            animated_textures: true,
            present_mode: PresentMode::Auto,
            pacing: Pacing::Efficient,
            workers: 0,
            overlay: Overlay::Off,
            resolution_scale: 100,
            fxaa: false,
            taa: false,
            upscaling: Upscaling::Native,
            hdr: false,
            hdr_paper_white: 0,
            hdr_peak: 0,
            hdr_black_level: 0,
            hdr_ui_brightness: 100,
            battery: false,
            api: GraphicsApi::Auto,
            shader_pack: String::new(),
        }
    }
}
fn bool_text(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}
impl Graphics {
    pub fn load(dir: &Path) -> Self {
        let mut g: Self = fs::read_to_string(dir.join(FILE))
            .map(|t| Self::parse(&t))
            .unwrap_or_default();
        if fs::remove_file(dir.join(HDR_MARKER)).is_ok() && g.hdr {
            g.hdr = false;
            g.save(dir);
            log::warn!(
                "the last start ended before HDR output showed a frame; HDR is off now (graphics option hdr, in {FILE})"
            );
        }
        g
    }
    pub fn parse(text: &str) -> Self {
        let mut g = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            let flag = |d: bool| match value {
                "true" => true,
                "false" => false,
                _ => d,
            };
            let number = |d: u32| value.parse().unwrap_or(d);
            match key.trim() {
                "preset" => g.preset = Preset::parse(value),
                "lodDistance" => g.lod_distance = number(g.lod_distance).min(1024),
                "farDetail" => g.far_detail = FarDetail::parse(value),
                "caveCulling" => g.cave_culling = flag(g.cave_culling),
                "entityCulling" => g.entity_culling = flag(g.entity_culling),
                "chunkUpdates" => g.chunk_updates = number(g.chunk_updates).min(512),
                "uploadBudgetKiB" => g.upload_budget_kib = number(g.upload_budget_kib).min(65536),
                "sky" => g.sky = flag(g.sky),
                "fog" => g.fog = flag(g.fog),
                "weather" => g.weather = flag(g.weather),
                "animatedTextures" => g.animated_textures = flag(g.animated_textures),
                "presentMode" => g.present_mode = PresentMode::parse(value),
                "framePacing" => {
                    g.pacing = if value == "precise" {
                        Pacing::Precise
                    } else {
                        Pacing::Efficient
                    }
                }
                "workers" => g.workers = number(g.workers).min(64),
                "resolutionScale" => g.resolution_scale = number(g.resolution_scale).clamp(50, 200),
                "fxaa" => g.fxaa = flag(g.fxaa),
                "taa" => g.taa = flag(g.taa),
                "upscaling" => g.upscaling = Upscaling::parse(value),
                "hdr" => g.hdr = flag(g.hdr),
                "hdrPaperWhite" => {
                    g.hdr_paper_white = match number(g.hdr_paper_white) {
                        0 => 0,
                        n => n.clamp(80, 480),
                    }
                }
                "hdrPeak" => {
                    g.hdr_peak = match number(g.hdr_peak) {
                        0 => 0,
                        n => n.clamp(200, 2000),
                    }
                }
                "hdrBlackLevel" => g.hdr_black_level = number(g.hdr_black_level).min(100),
                "hdrUiBrightness" => {
                    g.hdr_ui_brightness = number(g.hdr_ui_brightness).clamp(25, 100)
                }
                "batteryMode" => g.battery = flag(g.battery),
                "graphicsApi" => g.api = GraphicsApi::from_name(value).unwrap_or_default(),
                "pickedShaderPack" => g.shader_pack = value.to_owned(),
                "overlay" => {
                    g.overlay = match value {
                        "fps" => Overlay::Fps,
                        "graph" => Overlay::Graph,
                        _ => Overlay::Off,
                    }
                }
                _ => {}
            }
        }
        g
    }
    pub fn serialize(&self) -> String {
        format!(
            "preset:{}\nlodDistance:{}\nfarDetail:{}\ncaveCulling:{}\nentityCulling:{}\nchunkUpdates:{}\nuploadBudgetKiB:{}\nsky:{}\nfog:{}\nweather:{}\nanimatedTextures:{}\npresentMode:{}\nframePacing:{}\nworkers:{}\noverlay:{}\nresolutionScale:{}\nfxaa:{}\ntaa:{}\nupscaling:{}\nhdr:{}\nhdrPaperWhite:{}\nhdrPeak:{}\nhdrBlackLevel:{}\nhdrUiBrightness:{}\nbatteryMode:{}\ngraphicsApi:{}\npickedShaderPack:{}\n",
            self.preset.name(),
            self.lod_distance,
            self.far_detail.name(),
            bool_text(self.cave_culling),
            bool_text(self.entity_culling),
            self.chunk_updates,
            self.upload_budget_kib,
            bool_text(self.sky),
            bool_text(self.fog),
            bool_text(self.weather),
            bool_text(self.animated_textures),
            self.present_mode.name(),
            match self.pacing {
                Pacing::Efficient => "efficient",
                Pacing::Precise => "precise",
            },
            self.workers,
            match self.overlay {
                Overlay::Off => "off",
                Overlay::Fps => "fps",
                Overlay::Graph => "graph",
            },
            self.resolution_scale,
            bool_text(self.fxaa),
            bool_text(self.taa),
            self.upscaling.name(),
            bool_text(self.hdr),
            self.hdr_paper_white,
            self.hdr_peak,
            self.hdr_black_level,
            self.hdr_ui_brightness,
            bool_text(self.battery),
            self.api.name(),
            self.shader_pack,
        )
    }
    pub fn save(&self, dir: &Path) {
        let _ = fs::write(dir.join(FILE), self.serialize());
    }
    pub fn paper_white_nits(&self) -> u32 {
        match self.hdr_paper_white {
            0 => reported().map_or(203, |r| r.paper_white),
            n => n,
        }
    }
    pub fn peak_nits(&self) -> u32 {
        match self.hdr_peak {
            0 => reported().map_or(1000, |r| r.peak),
            n => n,
        }
    }
    pub fn worker_threads(&self) -> usize {
        if self.workers > 0 {
            return self.workers as usize;
        }
        let auto = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .saturating_sub(1)
            .max(1);
        if self.battery { auto.div_ceil(2) } else { auto }
    }
    pub fn battery_limit(&self, limit: i32, in_world: bool, focused: bool) -> i32 {
        if !self.battery {
            limit
        } else if in_world && focused {
            limit.min(60)
        } else {
            limit.min(30)
        }
    }
}

pub fn apply_preset(preset: Preset, s: &mut crate::settings::Settings) {
    let (rd, sim, clouds, particles, shadows, blend, mipmaps, entity, leaves, weather_r) =
        match preset {
            Preset::Performance => (8, 6, "false", 2, false, 0, 0, 0.5, false, 3),
            Preset::Balanced => (12, 8, "fast", 1, true, 1, 2, 0.75, true, 5),
            Preset::Quality => (16, 12, "true", 0, true, 2, 4, 1.0, true, 10),
            Preset::Ultra => (24, 12, "true", 0, true, 3, 4, 1.5, true, 10),
            Preset::Custom => return,
        };
    s.render_distance = rd;
    s.simulation_distance = sim;
    s.clouds = clouds.into();
    s.particles = particles;
    s.entity_shadows = shadows;
    s.biome_blend = blend;
    s.mipmap_levels = mipmaps;
    s.entity_distance = entity;
    s.cutout_leaves = leaves;
    s.weather_radius = weather_r;
    s.graphics_preset = "custom".into();
    let g = &mut s.vise;
    g.preset = preset;
    g.cave_culling = true;
    g.animated_textures = true;
    g.chunk_updates = 0;
    g.upload_budget_kib = 0;
    g.sky = true;
    g.fog = true;
    g.weather = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_api_choice_matches_the_system() {
        let choices = GraphicsApi::choices();
        if cfg!(windows) {
            assert_eq!(choices, GraphicsApi::ALL);
        } else if cfg!(target_os = "macos") {
            assert_eq!(choices, [GraphicsApi::Auto]);
        } else {
            assert_eq!(
                choices,
                [GraphicsApi::Auto, GraphicsApi::Vulkan, GraphicsApi::OpenGl]
            );
        }
    }

    #[test]
    fn round_trips() {
        let g = Graphics {
            preset: Preset::Balanced,
            lod_distance: 256,
            far_detail: FarDetail::Reduced,
            cave_culling: false,
            entity_culling: true,
            chunk_updates: 12,
            upload_budget_kib: 4096,
            sky: false,
            fog: false,
            weather: false,
            animated_textures: false,
            present_mode: PresentMode::Mailbox,
            pacing: Pacing::Precise,
            workers: 6,
            overlay: Overlay::Graph,
            resolution_scale: 75,
            fxaa: true,
            taa: true,
            upscaling: Upscaling::Balanced,
            hdr: true,
            hdr_paper_white: 240,
            hdr_peak: 1400,
            hdr_black_level: 12,
            hdr_ui_brightness: 60,
            battery: true,
            api: GraphicsApi::Dx12,
            shader_pack: "Golden Hour".into(),
        };
        assert_eq!(Graphics::parse(&g.serialize()), g);
        assert_eq!(Graphics::parse(""), Graphics::default());
        assert_eq!(Graphics::parse("hdrPeak:0\n").hdr_peak, 0);
        assert_eq!(Graphics::parse("hdrPeak:50\n").hdr_peak, 200);
        assert_eq!(Graphics::parse("hdrPaperWhite:0\n").hdr_paper_white, 0);
        assert_eq!(Graphics::parse("hdrPaperWhite:5\n").hdr_paper_white, 80);
        assert_eq!(Graphics::parse("level:advanced\n"), Graphics::default());
        assert_eq!(
            Graphics::parse("shaderPack:Pack.zip\n"),
            Graphics::default()
        );
        assert_eq!(
            Graphics::parse("workers:abc\nsky:maybe\ncullLeaves:true\nentityCulling:maybe\n"),
            Graphics::default()
        );
    }

    #[test]
    fn integrated_gpus_other_than_apple_cap_the_lod_distance() {
        let adapter = |name: &str, device_type| wgpu::AdapterInfo {
            name: name.into(),
            vendor: 0,
            device: 0,
            device_type,
            device_pci_bus_id: String::new(),
            driver: String::new(),
            driver_info: String::new(),
            backend: wgpu::Backend::Vulkan,
            subgroup_min_size: 0,
            subgroup_max_size: 0,
            transient_saves_memory: None,
            limit_bucket: None,
        };
        let hd620 = adapter(
            "Intel(R) HD Graphics 620 (KBL GT2)",
            wgpu::DeviceType::IntegratedGpu,
        );
        let m4 = adapter("Apple M4 Pro", wgpu::DeviceType::IntegratedGpu);
        let discrete = adapter("NVIDIA GeForce RTX 4070", wgpu::DeviceType::DiscreteGpu);
        for (distance, capped) in [(0, 0), (64, 64), (128, 128), (256, 128), (1024, 128)] {
            let g = Graphics {
                lod_distance: distance,
                ..Graphics::default()
            };
            assert_eq!(g.lod_distance_on(&hd620), capped);
            assert_eq!(g.lod_distance_on(&m4), distance);
            assert_eq!(g.lod_distance_on(&discrete), distance);
        }
        assert!(
            crate::graphics_ui::rows(crate::graphics_ui::Level::Simple)
                .iter()
                .any(|row| matches!(row, crate::graphics_ui::Row::Setting(s)
                    if s.text.contains(&format!("{INTEGRATED_LOD_DISTANCE} chunks")))),
            "the LOD Distance description names the cap"
        );
    }

    #[test]
    fn lod_distance_is_opt_in_and_bounded() {
        assert_eq!(Graphics::default().lod_distance, 0);
        for distance in [0, 64, 128, 256, 512, 1024] {
            let g = Graphics::parse(&format!("lodDistance:{distance}\n"));
            assert_eq!(g.lod_distance, distance);
            assert_eq!(Graphics::parse(&g.serialize()), g);
        }
        assert_eq!(Graphics::parse("lodDistance:10000\n").lod_distance, 1024);
        assert_eq!(Graphics::parse("lodDistance:-1\n").lod_distance, 0);
        assert_eq!(Graphics::parse("lodDistance:invalid\n").lod_distance, 0);
    }

    #[test]
    fn far_detail_is_full_unless_reduced_is_picked() {
        assert_eq!(Graphics::default().far_detail, FarDetail::Full);
        assert_eq!(
            Graphics::parse("farDetail:reduced\n").far_detail,
            FarDetail::Reduced
        );
        for text in ["farDetail:full\n", "farDetail:other\n", ""] {
            assert_eq!(Graphics::parse(text).far_detail, FarDetail::Full);
        }
        for preset in Preset::ALL {
            let mut s = crate::settings::Settings::default();
            apply_preset(preset, &mut s);
            assert_eq!(s.vise.far_detail, FarDetail::Full, "{preset:?}");
        }
    }

    #[test]
    fn every_preset_keeps_the_animated_textures_on() {
        assert!(Graphics::default().animated_textures);
        for preset in Preset::ALL {
            let mut s = crate::settings::Settings::default();
            apply_preset(preset, &mut s);
            assert!(
                s.vise.animated_textures,
                "{preset:?} turned the animated textures off"
            );
        }
    }

    #[test]
    fn automatic_workers_leave_a_core_as_vanilla_does() {
        let g = Graphics::default();
        assert_eq!(g.workers, 0);
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        assert_eq!(g.worker_threads(), cores.saturating_sub(1).max(1));
        let manual = Graphics { workers: 3, ..g };
        assert_eq!(manual.worker_threads(), 3);
    }

    #[test]
    fn battery_mode_only_lowers_the_frame_cap() {
        let mut g = Graphics::default();
        assert_eq!(g.battery_limit(260, true, true), 260);
        g.battery = true;
        assert_eq!(g.battery_limit(260, true, true), 60);
        assert_eq!(g.battery_limit(30, true, true), 30);
        assert_eq!(g.battery_limit(260, false, true), 30);
        assert_eq!(g.battery_limit(260, true, false), 30);
        assert_eq!(g.battery_limit(10, true, false), 10);
    }
}
