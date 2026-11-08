
use glam::{DMat4, DVec3, Mat4, Vec2};

const HISTORY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

struct WorldTarget {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    group: wgpu::BindGroup,
    taa: Option<History>,
}

struct History {
    size: [u32; 2],
    history: [wgpu::TextureView; 2],
    resolve: [wgpu::BindGroup; 2],
    show: [wgpu::BindGroup; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TaaUniform {
    reproject: [[f32; 4]; 4],
    jitter: [f32; 4],
}

struct TaaResources {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
}

pub struct Post {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    copy: wgpu::RenderPipeline,
    fxaa: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    pub scale: f32,
    pub antialias: bool,
    pub taa: bool,
    pub upscale: f32,
    taa_on: bool,
    target: Option<WorldTarget>,
    taa_resources: Option<TaaResources>,
    taa_frame: u32,
    taa_previous: Option<(DMat4, DVec3)>,
    taa_this_frame: TaaUniform,
}

impl Post {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vise post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../world_post.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Vise post bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let texture = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Vise post"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = |entry: &str, layout: &wgpu::PipelineLayout, format| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Vise post"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_post"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let taa_resources = {
            let taa_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Vise TAA bindings"),
                entries: &[
                    texture(0, float),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    texture(2, float),
                    texture(3, wgpu::TextureSampleType::Depth),
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
            let taa_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Vise TAA"),
                    bind_group_layouts: &[Some(&taa_layout)],
                    ..Default::default()
                });
            let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
            let taa_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Vise TAA"),
                layout: Some(&taa_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_post"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_taa"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HISTORY_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
            let internal = pollster::block_on(internal.pop());
            match internal.or(pollster::block_on(validation.pop())) {
                None => {
                    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Vise TAA"),
                        size: std::mem::size_of::<TaaUniform>() as u64,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    Some(TaaResources {
                        layout: taa_layout,
                        pipeline: taa_pipeline,
                        uniform,
                    })
                }
                Some(e) => {
                    log::warn!("Vise renderer: temporal anti-aliasing unavailable ({e})");
                    None
                }
            }
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Vise post"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            copy: pipeline("fs_copy", &pipeline_layout, format),
            fxaa: pipeline("fs_fxaa", &pipeline_layout, format),
            layout,
            sampler,
            format,
            scale: 1.0,
            antialias: false,
            taa: false,
            upscale: 1.0,
            taa_on: false,
            target: None,
            taa_resources,
            taa_frame: 0,
            taa_previous: None,
            taa_this_frame: bytemuck::Zeroable::zeroed(),
        }
    }

    pub fn supports_taa(&self) -> bool {
        self.taa_resources.is_some()
    }

    fn sizes(&self, window: [u32; 2]) -> ([u32; 2], [u32; 2]) {
        let output = window.map(|v| ((v as f32 * self.scale).round() as u32).clamp(1, 8192));
        let upscale = if self.taa_on { self.upscale } else { 1.0 };
        (
            output.map(|v| ((v as f32 / upscale).round() as u32).max(1)),
            output,
        )
    }

    pub fn jitter(&mut self, window: [u32; 2], on: bool, view_proj: Mat4, camera: DVec3) -> Mat4 {
        self.taa_on = on && self.taa && self.taa_resources.is_some();
        if !self.taa_on {
            self.taa_previous = None;
            return Mat4::IDENTITY;
        }
        self.taa_frame = self.taa_frame.wrapping_add(1);
        let (size, _) = self.sizes(window);
        let phases = (8.0 * self.upscale * self.upscale).ceil() as u32;
        let i = self.taa_frame % phases + 1;
        let offset = Vec2::new(halton(i, 2), halton(i, 3)) - 0.5;
        let jitter = offset * 2.0 / Vec2::new(size[0] as f32, size[1] as f32);
        let current = view_proj.as_dmat4();
        let (reproject, reset) = match self.taa_previous {
            Some((previous, previous_camera)) => (
                reprojection(previous, previous_camera, current, camera),
                0.0,
            ),
            None => (DMat4::IDENTITY, 1.0),
        };
        self.taa_previous = Some((current, camera));
        self.taa_this_frame = TaaUniform {
            reproject: reproject.as_mat4().to_cols_array_2d(),
            jitter: [jitter.x, jitter.y, reset, 0.0],
        };
        Mat4::from_translation(jitter.extend(0.0))
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        window: [u32; 2],
        always: bool,
    ) -> Option<(wgpu::TextureView, wgpu::TextureView)> {
        if self.scale == 1.0 && !self.antialias && !self.taa_on && !always {
            self.target = None;
            return None;
        }
        let (size, output) = self.sizes(window);
        let history_size = self.taa_on.then_some(output);
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.size != size || t.taa.as_ref().map(|h| h.size) != history_size)
        {
            let make_tex = |label, size: [u32; 2], format, usage| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let sampled =
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
            let color = make_tex("Scaled world", size, self.format, sampled)
                .create_view(&Default::default());
            let depth = make_tex(
                "Scaled world depth",
                size,
                wgpu::TextureFormat::Depth32Float,
                sampled,
            )
            .create_view(&Default::default());
            let group = self.bind(device, &color);
            let taa = history_size.and_then(|output| {
                let res = self.taa_resources.as_ref()?;
                let history = [0, 1].map(|_| {
                    make_tex("TAA history", output, HISTORY_FORMAT, sampled)
                        .create_view(&Default::default())
                });
                let resolve = [0, 1].map(|i| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Vise TAA"),
                        layout: &res.layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&color),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.sampler),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&history[1 - i]),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&depth),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: res.uniform.as_entire_binding(),
                            },
                        ],
                    })
                });
                let show = [0, 1].map(|i| self.bind(device, &history[i]));
                Some(History {
                    size: output,
                    history,
                    resolve,
                    show,
                })
            });
            self.taa_this_frame.jitter[2] = 1.0;
            self.target = Some(WorldTarget {
                size,
                color,
                depth,
                group,
                taa,
            });
        }
        self.target
            .as_ref()
            .map(|t| (t.color.clone(), t.depth.clone()))
    }

    pub fn size(&self) -> Option<[u32; 2]> {
        self.target.as_ref().map(|t| t.size)
    }

    pub fn bind(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Vise post"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    pub fn finish(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        screen: &wgpu::TextureView,
    ) {
        let Some(target) = &self.target else { return };
        let Some(taa) = target.taa.as_ref().filter(|_| self.taa_previous.is_some()) else {
            self.finish_with(encoder, screen, &target.group);
            return;
        };
        let Some(res) = &self.taa_resources else {
            self.finish_with(encoder, screen, &target.group);
            return;
        };
        let i = (self.taa_frame % 2) as usize;
        queue.write_buffer(&res.uniform, 0, bytemuck::bytes_of(&self.taa_this_frame));
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &taa.history[i],
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Vise TAA"),
            color_attachments: &attachments,
            ..Default::default()
        });
        pass.set_pipeline(&res.pipeline);
        pass.set_bind_group(0, &taa.resolve[i], &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        self.finish_with(encoder, screen, &taa.show[i]);
    }

    pub fn finish_with(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        screen: &wgpu::TextureView,
        group: &wgpu::BindGroup,
    ) {
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: screen,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Vise post"),
            color_attachments: &attachments,
            ..Default::default()
        });
        pass.set_pipeline(if self.antialias {
            &self.fxaa
        } else {
            &self.copy
        });
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn reprojection(previous: DMat4, previous_camera: DVec3, current: DMat4, camera: DVec3) -> DMat4 {
    previous * DMat4::from_translation(camera - previous_camera) * current.inverse()
}

fn halton(mut i: u32, base: u32) -> f32 {
    let (mut f, mut r) = (1.0, 0.0);
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reprojection_finds_where_a_block_was_last_frame() {
        let projection = DMat4::perspective_rh(1.2, 16.0 / 9.0, 1024.0, 0.05);
        let previous = projection * DMat4::from_rotation_y(0.3);
        let current = projection * DMat4::from_rotation_y(0.35);
        let (previous_camera, camera) = (DVec3::new(10.0, 64.0, 5.0), DVec3::new(10.5, 64.2, 4.0));
        let block = DVec3::new(14.0, 66.0, -8.0);
        let clip = |m: DMat4, eye: DVec3| m * (block - eye).extend(1.0);
        let now = clip(current, camera);
        let then = reprojection(previous, previous_camera, current, camera) * (now / now.w);
        let expected = clip(previous, previous_camera);
        assert!((then.truncate() / then.w - expected.truncate() / expected.w).length() < 1e-9);
    }

    #[test]
    fn jitter_stays_inside_the_pixel() {
        for i in 1..=8 {
            let (x, y) = (halton(i, 2), halton(i, 3));
            assert!((0.0..1.0).contains(&x) && (0.0..1.0).contains(&y));
        }
        assert_eq!(
            [halton(1, 2), halton(2, 2), halton(3, 2)],
            [0.5, 0.25, 0.75]
        );
    }

    fn test_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).ok()?;
        pollster::block_on(adapter.request_device(&Default::default())).ok()
    }

    #[test]
    fn world_post_pipelines_compile_or_fall_back() {
        let Some((device, _)) = test_device() else {
            return;
        };
        let post = Post::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let _ = post.supports_taa();
    }
}
