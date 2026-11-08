
use vise_shader_api::{Report, StageKind};

use super::native_material::{NativeMaterial, TerrainShape};
use super::native_post::{NativeFrame, NativePost};
use super::native_sky::NativeSky;
use super::pack_timing::PassTiming;

pub struct NativePack {
    pub frame: NativeFrame,
    pub sky: Option<NativeSky>,
    pub material: Option<NativeMaterial>,
    pub post: Option<NativePost>,
    pub timing: Option<PassTiming>,
}

impl NativePack {
    pub fn new(
        device: &wgpu::Device,
        terrain: &TerrainShape<'_>,
        report: Report,
        constants: &[(String, f64)],
    ) -> Result<(Self, Vec<String>), String> {
        let (globals, format) = (terrain.globals, terrain.format);
        let frame = NativeFrame::new(device);
        let (post, others): (Vec<_>, Vec<_>) = report
            .stages
            .into_iter()
            .partition(|s| s.kind == StageKind::Post);
        let sky = others
            .iter()
            .find(|s| s.kind == StageKind::Sky)
            .map(|stage| NativeSky::new(device, globals, &frame, stage, constants, format))
            .transpose()?;
        let material = others
            .iter()
            .find(|s| s.kind == StageKind::Material)
            .map(|stage| NativeMaterial::new(device, terrain, &frame, stage, constants))
            .transpose()?;
        let post = (!post.is_empty())
            .then(|| NativePost::new(device, post, constants, format, &frame))
            .transpose()?;
        let mut labels: Vec<String> = sky.iter().map(|s| s.label.clone()).collect();
        labels.extend(
            others
                .iter()
                .filter(|s| material.is_some() && s.kind == StageKind::Material)
                .map(|s| s.label.clone()),
        );
        if let Some(post) = &post {
            labels.extend(post.labels().into_iter().map(str::to_owned));
        }
        let pack = Self {
            frame,
            sky,
            material,
            post,
            timing: None,
        };
        Ok((pack, labels))
    }

    pub fn draws(&self) -> bool {
        self.sky.is_some() || self.material.is_some() || self.post.is_some()
    }
}
