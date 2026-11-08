
use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const REPORT: usize = 30;
const CAPACITY: u32 = 64;
const WINDOW: usize = 15;

enum Stage {
    Idle,
    Recording,
    Running(Arc<AtomicBool>),
    Reading(Arc<AtomicBool>, u64),
}

pub struct PassTiming {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    period_ns: f32,
    stage: Stage,
    names: Vec<String>,
    samples: Vec<(String, VecDeque<f32>)>,
    log: bool,
    marker: Option<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    reports: usize,
}

impl PassTiming {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let size = u64::from(CAPACITY) * 2 * 8;
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        Some(Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("Shader pack pass timing"),
                ty: wgpu::QueryType::Timestamp,
                count: CAPACITY * 2,
            }),
            resolve: buffer(
                "Shader pack timing resolve",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            read: buffer(
                "Shader pack timing read",
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            ),
            period_ns: queue.get_timestamp_period(),
            stage: Stage::Idle,
            names: Vec::new(),
            samples: Vec::new(),
            log: Self::logging(),
            marker: None,
            reports: 0,
        })
    }

    pub fn logging() -> bool {
        static LOG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *LOG.get_or_init(|| std::env::var_os("VISE_SHADER_PACK_TIMING").is_some())
    }

    pub fn begin(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        match &self.stage {
            Stage::Recording => self.stage = Stage::Idle,
            Stage::Running(done) if done.load(Ordering::Acquire) => {
                let bytes = self.names.len() as u64 * 16;
                let mut encoder = device.create_command_encoder(&Default::default());
                encoder.resolve_query_set(
                    &self.queries,
                    0..self.names.len() as u32 * 2,
                    &self.resolve,
                    0,
                );
                encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, bytes);
                queue.submit([encoder.finish()]);
                let mapped = Arc::new(AtomicBool::new(false));
                let flag = mapped.clone();
                self.read
                    .slice(..bytes)
                    .map_async(wgpu::MapMode::Read, move |r| {
                        flag.store(r.is_ok(), Ordering::Release)
                    });
                self.stage = Stage::Reading(mapped, bytes);
            }
            Stage::Reading(mapped, bytes) if mapped.load(Ordering::Acquire) => {
                let bytes = *bytes;
                self.collect(bytes);
                self.stage = Stage::Idle;
            }
            _ => {}
        }
        if matches!(self.stage, Stage::Idle) {
            self.names.clear();
            self.stage = Stage::Recording;
        }
    }

    pub fn recording(&self) -> bool {
        matches!(self.stage, Stage::Recording)
    }

    pub fn writes(&mut self, name: &str) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let i = self.push(name)?;
        Some(stamps(&self.queries, i))
    }

    fn push(&mut self, name: &str) -> Option<u32> {
        let i = self.names.len() as u32;
        if !matches!(self.stage, Stage::Recording) || i >= CAPACITY {
            return None;
        }
        self.names.push(name.to_owned());
        Some(i)
    }

    pub fn mark(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (target, format): super::timing::Target,
    ) {
        if self.marker.as_ref().is_none_or(|(f, _)| *f != format) {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Shader pack timing mark"),
                source: wgpu::ShaderSource::Wgsl(super::timing::MARKER.into()),
            });
            let pipeline = super::timing::end_pipeline(device, &module, format);
            self.marker = Some((format, pipeline));
        }
        let Some(i) = self.push("") else {
            return;
        };
        let (_, pipeline) = self.marker.as_ref().expect("made above");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shader pack timing mark"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            timestamp_writes: Some(stamps(&self.queries, i)),
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_scissor_rect(0, 0, 1, 1);
        pass.draw(0..3, 0..1);
    }

    pub fn submitted(&mut self, queue: &wgpu::Queue) {
        if !matches!(self.stage, Stage::Recording) {
            return;
        }
        if self.names.is_empty() {
            self.stage = Stage::Idle;
            return;
        }
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        queue.on_submitted_work_done(move || flag.store(true, Ordering::Release));
        self.stage = Stage::Running(done);
    }

    fn collect(&mut self, bytes: u64) {
        let slice = self.read.slice(..bytes);
        let Ok(data) = slice.get_mapped_range() else {
            return;
        };
        let stamps: Vec<u64> = data
            .as_chunks::<8>()
            .0
            .iter()
            .map(|b| u64::from_le_bytes(*b))
            .collect();
        drop(data);
        self.read.unmap();
        let times = pass_times(&stamps, self.period_ns);
        for (name, ms) in self.names.iter().zip(times) {
            let Some(ms) = ms.filter(|_| !name.is_empty()) else {
                continue;
            };
            let i = match self.samples.iter().position(|s| s.0 == *name) {
                Some(i) => i,
                None => {
                    self.samples.push((name.clone(), VecDeque::new()));
                    self.samples.len() - 1
                }
            };
            let window = &mut self.samples[i].1;
            if window.len() == WINDOW {
                window.pop_front();
            }
            window.push_back(ms);
        }
        self.reports += 1;
        if self.log && self.reports.is_multiple_of(REPORT) {
            let medians = self.medians();
            let total: f32 = medians.iter().map(|m| m.1).sum();
            let parts: Vec<String> = medians
                .iter()
                .map(|(n, ms)| format!("{n} {ms:.3}"))
                .collect();
            log::info!(
                "shader pack GPU time per pass (median ms of the last {WINDOW} timed frames): {} = {total:.3}",
                parts.join(", ")
            );
        }
    }

    pub fn medians(&self) -> Vec<(String, f32)> {
        self.samples
            .iter()
            .filter(|s| !s.1.is_empty())
            .map(|(name, v)| {
                let mut v: Vec<f32> = v.iter().copied().collect();
                v.sort_by(f32::total_cmp);
                (name.clone(), v[v.len() / 2])
            })
            .collect()
    }
}

fn stamps(queries: &wgpu::QuerySet, i: u32) -> wgpu::RenderPassTimestampWrites<'_> {
    wgpu::RenderPassTimestampWrites {
        query_set: queries,
        beginning_of_pass_write_index: Some(2 * i),
        end_of_pass_write_index: Some(2 * i + 1),
    }
}

fn pass_times(stamps: &[u64], period_ns: f32) -> Vec<Option<f32>> {
    let mut previous_end = 0;
    stamps
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[begin, end]| {
            if begin == 0 || end < begin {
                return None;
            }
            let from = begin.max(previous_end).min(end);
            previous_end = end;
            Some((end - from) as f32 * period_ns / 1e6)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::pass_times;

    #[test]
    fn overlapping_passes_cost_from_the_previous_end() {
        let stamps = [1_000_000, 3_000_000, 2_000_000, 5_000_000, 0, 0];
        assert_eq!(pass_times(&stamps, 1.0), [Some(2.0), Some(2.0), None]);
    }
}
