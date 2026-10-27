//! Куда рисуется кадр: сглаживание, глубина и готовая картинка.

use super::{Gpu, COLOR_FORMAT, DEPTH_FORMAT, SAMPLES};

/// Тот же кадр для окна: egui ждёт в картинке цвета sRGB как есть, без
/// пересчёта, поэтому окну отдаётся вид той же памяти без пометки sRGB.
const SHOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Текстуры кадра одного размера.
pub struct Target {
    pub size: [u32; 2],
    pub msaa: wgpu::TextureView,
    pub depth: wgpu::TextureView,
    pub color: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub shown: wgpu::TextureView,
}

impl Gpu {
    pub(super) fn make_target(&self, size: [u32; 2]) -> Target {
        let texture = |label, format, samples, usage, view_formats: &[wgpu::TextureFormat]| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats,
            })
        };
        use wgpu::TextureUsages as U;
        let msaa = texture("msaa", COLOR_FORMAT, SAMPLES, U::RENDER_ATTACHMENT, &[]);
        let depth = texture("depth", DEPTH_FORMAT, SAMPLES, U::RENDER_ATTACHMENT, &[]);
        let usage = U::RENDER_ATTACHMENT | U::TEXTURE_BINDING | U::COPY_SRC;
        let color = texture("view", COLOR_FORMAT, 1, usage, &[SHOW_FORMAT]);
        Target {
            size,
            msaa: msaa.create_view(&Default::default()),
            depth: depth.create_view(&Default::default()),
            view: color.create_view(&Default::default()),
            shown: color.create_view(&wgpu::TextureViewDescriptor {
                format: Some(SHOW_FORMAT),
                ..Default::default()
            }),
            color,
        }
    }
}
