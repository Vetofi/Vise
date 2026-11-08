
use vise_shaders::{RENDER_STAGES, render_stage};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntitySpan {
    pub start: u32,
    pub stage: u32,
    pub id: i32,
    pub translucent: bool,
    pub hidden: bool,
    pub player: bool,
    pub emissive: bool,
}

impl EntitySpan {
    pub fn hide(spans: &mut Vec<EntitySpan>, start: usize, end: usize) -> bool {
        let Some(i) = spans.iter().rposition(|s| s.start as usize <= start) else {
            return false;
        };
        let after = spans[spans.len() - 1];
        if spans[i].start as usize != start {
            let split = EntitySpan {
                start: start as u32,
                ..spans[i]
            };
            spans.insert(i + 1, split);
        }
        for s in spans.iter_mut().skip(i) {
            if s.start as usize >= start {
                s.hidden = true;
            }
        }
        if end > start {
            spans.push(EntitySpan {
                start: end as u32,
                ..after
            });
        }
        true
    }
}

const SIZE: u32 = 32;

pub(super) struct DrawUniforms {
    pub layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    stride: u32,
}

impl DrawUniforms {
    pub fn new(device: &wgpu::Device) -> Self {
        let stride = SIZE.max(device.limits().min_uniform_buffer_offset_alignment);
        let mut contents = vec![0u8; stride as usize * RENDER_STAGES.len()];
        for (i, slot) in contents.chunks_mut(stride as usize).enumerate() {
            slot[..4].copy_from_slice(&(i as i32).to_le_bytes());
        }
        let buffer = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("Shader pack draws"),
                contents: &contents,
                usage: wgpu::BufferUsages::UNIFORM,
            },
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shader pack draws"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(u64::from(SIZE)),
                },
                count: None,
            }],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shader pack draws"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(u64::from(SIZE)),
                }),
            }],
        });
        Self {
            layout,
            group,
            stride,
        }
    }

    pub fn bind(&self, pass: &mut wgpu::RenderPass<'_>, stage: &str) {
        pass.set_bind_group(2, &self.group, &[self.stride * render_stage(stage)]);
    }
}

#[cfg(test)]
mod tests {
    use super::EntitySpan;

    fn span(start: u32, id: i32) -> EntitySpan {
        EntitySpan {
            start,
            id,
            ..Default::default()
        }
    }

    #[test]
    fn hide_splits_the_span_around_one_entity() {
        let mut spans = vec![span(0, 7)];
        assert!(EntitySpan::hide(&mut spans, 8, 16));
        let hidden = EntitySpan {
            hidden: true,
            ..span(8, 7)
        };
        assert_eq!(spans, [span(0, 7), hidden, span(16, 7)]);
        let mut spans = vec![span(0, 1), span(4, 2), span(6, 3)];
        assert!(EntitySpan::hide(&mut spans, 4, 10));
        assert!(!spans[0].hidden && spans[1].hidden && spans[2].hidden);
        assert_eq!(spans[3], span(10, 3));
        assert!(!EntitySpan::hide(&mut Vec::new(), 0, 4));
    }
}
