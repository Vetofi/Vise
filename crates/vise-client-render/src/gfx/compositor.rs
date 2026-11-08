use super::Renderer;
use crate::Result;
use std::sync::Arc;
use winit::window::Window;

pub struct Compositor {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    present_modes: Vec<wgpu::PresentMode>,
    suspended: bool,
}

impl Renderer {
    pub fn compositor(&self, window: Arc<Window>) -> Result<Compositor> {
        let spare = self
            .spare_surface
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let surface = match spare {
            Some(surface) => surface,
            None => self.instance.create_surface(window.clone())?,
        };
        Compositor::new(
            surface,
            &self.adapter,
            &self.device,
            &self.queue,
            window.inner_size().into(),
        )
    }

    pub fn offscreen_frame(&self) -> Option<&wgpu::Texture> {
        self.offscreen.as_ref()
    }

    pub fn take_back(&self, compositor: Compositor) {
        *self
            .spare_surface
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(compositor.surface);
    }
}

impl Compositor {
    pub fn new(
        surface: wgpu::Surface<'static>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
    ) -> Result<Self> {
        let caps = surface.get_capabilities(adapter);
        if !caps.usages.contains(wgpu::TextureUsages::COPY_DST) {
            return Err("surface does not support copying offscreen frames".into());
        }
        let format = wgpu::TextureFormat::Bgra8Unorm;
        if !caps.formats.contains(&format) {
            return Err(format!(
                "splitscreen needs a {format:?} surface; this one offers {:?}",
                caps.formats
            )
            .into());
        }
        let max = device.limits().max_texture_dimension_2d;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            format,
            width: size[0].clamp(1, max),
            height: size[1].clamp(1, max),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::default(),
        };
        surface.configure(device, &config);
        Ok(Compositor {
            surface,
            device: device.clone(),
            queue: queue.clone(),
            config,
            present_modes: caps.present_modes,
            suspended: !super::drawable_size(size[0], size[1], max),
        })
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        let mode = [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate]
            .into_iter()
            .find(|m| !vsync && self.present_modes.contains(m))
            .unwrap_or(wgpu::PresentMode::Fifo);
        if mode != self.config.present_mode {
            self.config.present_mode = mode;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.suspended =
            !super::drawable_size(width, height, self.device.limits().max_texture_dimension_2d);
        if !self.suspended {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn present(&mut self, views: &[(&wgpu::Texture, [u32; 2])]) -> bool {
        if self.suspended {
            return false;
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return false;
            }
            _ => return false,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Splitscreen compositor"),
            });
        let target = frame.texture.create_view(&Default::default());
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Splitscreen compositor clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        }));
        for &(texture, [x, y]) in views {
            let [width, height] = clipped_size(
                [texture.width(), texture.height()],
                [x, y],
                [self.config.width, self.config.height],
            );
            if width == 0 || height == 0 {
                continue;
            }
            encoder.copy_texture_to_texture(
                texture.as_image_copy(),
                wgpu::TexelCopyTextureInfo {
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    ..frame.texture.as_image_copy()
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        true
    }
}

fn clipped_size(view: [u32; 2], at: [u32; 2], surface: [u32; 2]) -> [u32; 2] {
    [
        view[0].min(surface[0].saturating_sub(at[0])),
        view[1].min(surface[1].saturating_sub(at[1])),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_clipped_to_the_window_during_resize() {
        assert_eq!(clipped_size([640, 480], [0, 0], [1280, 720]), [640, 480]);
        assert_eq!(
            clipped_size([640, 480], [1000, 600], [1280, 720]),
            [280, 120]
        );
        assert_eq!(clipped_size([640, 480], [1280, 720], [1280, 720]), [0, 0]);
        assert_eq!(
            clipped_size([640, 480], [u32::MAX, 0], [1280, 720]),
            [0, 480]
        );
    }
}
