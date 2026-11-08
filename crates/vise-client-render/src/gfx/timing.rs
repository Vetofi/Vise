use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

const SLOTS: usize = 8;
const RESOLVE_STRIDE: u64 = wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;
pub type Target<'a> = (&'a wgpu::TextureView, wgpu::TextureFormat);
const FREE: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;
const PENDING: u8 = 3;
const MAPPED: u8 = 4;

struct Slot {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
}

pub struct GpuTimer {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<Slot>,
    current: Option<usize>,
    next: usize,
    period_ns: f32,
    markers: Option<Markers>,
    resolved: Vec<usize>,
}

struct Markers {
    module: wgpu::ShaderModule,
    start: wgpu::ComputePipeline,
    end: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
}

pub(super) const MARKER: &str =
    "@vertex fn cover(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32(i & 1u) * 4.0 - 1.0, f32(i >> 1u) * 4.0 - 1.0, 0.0, 1.0);
}
@fragment fn nothing() -> @location(0) vec4<f32> { return vec4<f32>(0.0); }
@compute @workgroup_size(1) fn mark() {}";

impl GpuTimer {
    pub fn features(adapter: &wgpu::Adapter) -> wgpu::Features {
        adapter.features()
            & (wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS)
    }
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("Frame timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: 2 * SLOTS as u32,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Frame timestamp resolve"),
            size: RESOLVE_STRIDE * SLOTS as u64,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = (0..SLOTS)
            .map(|_| Slot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Frame timestamp readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU8::new(FREE)),
            })
            .collect();
        Some(Self {
            queries,
            resolve,
            slots,
            current: None,
            next: 0,
            period_ns: queue.get_timestamp_period(),
            markers: (!device
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS))
            .then(|| {
                let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Frame timestamp"),
                    source: wgpu::ShaderSource::Wgsl(MARKER.into()),
                });
                Markers {
                    start: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some("Frame timestamp"),
                        layout: None,
                        module: &module,
                        entry_point: Some("mark"),
                        compilation_options: Default::default(),
                        cache: None,
                    }),
                    module,
                    end: None,
                }
            }),
            resolved: Vec::new(),
        })
    }
    pub fn begin(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.markers.is_some() {
            for slot in 0..SLOTS {
                if self.slots[slot].state.load(Ordering::Acquire) == DONE {
                    self.resolve(encoder, slot);
                }
            }
        }
        let slot = self.next;
        self.current = (self.slots[slot].state.load(Ordering::Acquire) == FREE).then_some(slot);
        let Some(slot) = self.current else { return };
        let index = 2 * slot as u32;
        let Some(markers) = &self.markers else {
            encoder.write_timestamp(&self.queries, index);
            return;
        };
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Frame timestamp"),
            timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: Some(index),
                end_of_pass_write_index: None,
            }),
        });
        pass.set_pipeline(&markers.start);
        pass.dispatch_workgroups(1, 1, 1);
    }
    pub fn end(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (target, format): Target,
    ) {
        let Some(slot) = self.current else { return };
        let index = 2 * slot as u32 + 1;
        let Some(markers) = &mut self.markers else {
            encoder.write_timestamp(&self.queries, index);
            self.resolve(encoder, slot);
            return;
        };
        if markers.end.as_ref().is_none_or(|(f, _)| *f != format) {
            markers.end = Some((format, end_pipeline(device, &markers.module, format)));
        }
        let (_, pipeline) = markers.end.as_ref().expect("made above");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Frame timestamp"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: None,
                end_of_pass_write_index: Some(index),
            }),
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_scissor_rect(0, 0, 1, 1);
        pass.draw(0..3, 0..1);
    }
    fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder, slot: usize) {
        let (first, at) = (2 * slot as u32, RESOLVE_STRIDE * slot as u64);
        encoder.resolve_query_set(&self.queries, first..first + 2, &self.resolve, at);
        encoder.copy_buffer_to_buffer(&self.resolve, at, &self.slots[slot].buffer, 0, 16);
        self.slots[slot].state.store(RUNNING, Ordering::Release);
        self.resolved.push(slot);
    }
    pub fn submitted(&mut self, queue: &wgpu::Queue) {
        for slot in self.resolved.drain(..) {
            let state = self.slots[slot].state.clone();
            state.store(PENDING, Ordering::Release);
            self.slots[slot]
                .buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    state.store(
                        if result.is_ok() { MAPPED } else { FREE },
                        Ordering::Release,
                    );
                });
        }
        let Some(slot) = self.current.take() else {
            return;
        };
        if self.markers.is_some() {
            let state = self.slots[slot].state.clone();
            state.store(RUNNING, Ordering::Release);
            queue.on_submitted_work_done(move || state.store(DONE, Ordering::Release));
        }
        self.next = (slot + 1) % SLOTS;
    }
    pub fn collect(&mut self, device: &wgpu::Device, out: &mut Vec<f32>) {
        let _ = device.poll(wgpu::PollType::Poll);
        for i in 0..SLOTS {
            let slot = &self.slots[(self.next + i) % SLOTS];
            if slot.state.load(Ordering::Acquire) != MAPPED {
                continue;
            }
            if let Ok(data) = slot.buffer.slice(..).get_mapped_range() {
                let stamps: [u64; 2] = bytemuck::pod_read_unaligned(&data[..16]);
                if let Some(ticks) = stamps[1].checked_sub(stamps[0]) {
                    out.push((ticks as f64 * f64::from(self.period_ns) / 1e6) as f32);
                }
            }
            slot.buffer.unmap();
            slot.state.store(FREE, Ordering::Release);
        }
    }
}

pub(super) fn end_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Frame timestamp"),
        layout: None,
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("cover"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("nothing"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
