
type Ops<T> = wgpu::Operations<T>;
type Color = wgpu::Color;

pub(super) fn load<T>() -> Ops<T> {
    wgpu::Operations {
        load: wgpu::LoadOp::Load,
        store: wgpu::StoreOp::Store,
    }
}

fn clear(c: [f32; 4]) -> Ops<Color> {
    wgpu::Operations {
        load: wgpu::LoadOp::Clear(wgpu::Color {
            r: c[0].into(),
            g: c[1].into(),
            b: c[2].into(),
            a: c[3].into(),
        }),
        store: wgpu::StoreOp::Store,
    }
}

pub(super) struct ShadowTargets {
    pub size: u32,
    pub depth: (wgpu::Texture, wgpu::TextureView),
    pub depth1: Option<(wgpu::Texture, wgpu::TextureView)>,
    pub color: [Option<[wgpu::TextureView; 2]>; 2],
    pub clears: [Option<[f32; 4]>; 2],
    pub main: u16,
    fresh: bool,
    cleared: u8,
    pub comparison: wgpu::Sampler,
}

fn target(
    device: &wgpu::Device,
    label: &str,
    size: u32,
    format: wgpu::TextureFormat,
    copy: wgpu::TextureUsages,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | copy,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    (texture, view)
}

impl ShadowTargets {
    pub fn new(
        device: &wgpu::Device,
        shadow: &vise_shaders::Shadow,
        formats: [wgpu::TextureFormat; 2],
        colors: u16,
        depth1: bool,
    ) -> Self {
        let size = shadow
            .resolution
            .clamp(1, device.limits().max_texture_dimension_2d);
        let copy = wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST;
        let depth = wgpu::TextureFormat::Depth32Float;
        let color = std::array::from_fn(|n| {
            (colors & 1 << n != 0).then(|| {
                [0, 1].map(|side| {
                    let label = format!("Shader pack shadowcolor{n} {}", ["A", "B"][side]);
                    target(
                        device,
                        &label,
                        size,
                        formats[n],
                        wgpu::TextureUsages::empty(),
                    )
                    .1
                })
            })
        });
        Self {
            size,
            depth: target(device, "Shader pack shadowtex0", size, depth, copy),
            depth1: depth1.then(|| target(device, "Shader pack shadowtex1", size, depth, copy)),
            color,
            clears: std::array::from_fn(|n| {
                let b = &shadow.colors[n];
                b.clear.then(|| b.clear_color.unwrap_or([1.0; 4]))
            }),
            main: 0,
            fresh: false,
            cleared: 0,
            comparison: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Shader pack shadow comparison"),
                compare: Some(wgpu::CompareFunction::LessEqual),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
        }
    }

    pub fn color(&self, n: usize, alt: bool) -> Option<&wgpu::TextureView> {
        let side = usize::from(self.main & 1 << n != 0) ^ usize::from(alt);
        self.color.get(n)?.as_ref().map(|c| &c[side])
    }

    pub fn begin_frame(&mut self) {
        self.fresh = true;
        self.cleared = 0;
    }

    pub fn clear_ops(
        &mut self,
        writes: impl Iterator<Item = usize>,
    ) -> (Ops<f32>, [Ops<Color>; 2]) {
        if !std::mem::take(&mut self.fresh) {
            return (load(), [load(); 2]);
        }
        let mut colors = [load(); 2];
        for n in writes.filter(|&n| n < 2) {
            if let Some(c) = self.clears[n] {
                colors[n] = clear(c);
                self.cleared |= 1 << n;
            }
        }
        let depth = wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Store,
        };
        (depth, colors)
    }

    pub fn clear_rest(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let depth = std::mem::take(&mut self.fresh);
        let cleared = std::mem::replace(&mut self.cleared, 0b11);
        let attachments: Vec<_> = (0..2)
            .filter(|n| cleared & 1 << n == 0)
            .filter_map(|n| {
                Some(Some(wgpu::RenderPassColorAttachment {
                    view: self.color(n, false)?,
                    depth_slice: None,
                    resolve_target: None,
                    ops: clear(self.clears[n]?),
                }))
            })
            .collect();
        if !depth && attachments.is_empty() {
            return;
        }
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shader pack shadow clear"),
            color_attachments: &attachments,
            depth_stencil_attachment: depth.then_some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth.1,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
    }

    pub fn copy_depth1(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some((copy, _)) = &self.depth1 {
            encoder.copy_texture_to_texture(
                self.depth.0.as_image_copy(),
                copy.as_image_copy(),
                self.depth.0.size(),
            );
        }
    }
}
