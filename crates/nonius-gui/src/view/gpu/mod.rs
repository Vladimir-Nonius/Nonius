//! Отрисовка на видеокарте (wgpu) в картинку, которую окно показывает как
//! обычное изображение. Та же отрисовка сохраняет PNG и работает в тестах
//! без окна.

mod pipelines;
mod readback;
mod target;

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

pub use readback::Image;

use super::mesh::{Part, Vertex};
use pipelines::{Pipelines, DRAW_SIZE};
use target::Target;

/// Формат картинки: 8 бит на канал, цвета в sRGB.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Формат глубины.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Сглаживание краёв: 4 выборки на пиксель.
pub const SAMPLES: u32 = 4;

/// Фон вида (sRGB).
pub const BACKGROUND: [u8; 3] = [236, 239, 243];

/// Как рисовать кусок.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Сплошной цвет.
    Solid,
    /// Сплошной, но чуть дальше от зрителя: совпадающие грани мест изменений
    /// рисуются поверх серой детали без ряби.
    Behind,
    /// Полупрозрачный, только ближайшая поверхность («рентген»).
    Ghost,
}

/// Один кусок в кадре.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Draw {
    /// Номер куска в загруженной сцене.
    pub part: usize,
    /// Цвет sRGB и непрозрачность.
    pub color: [u8; 4],
    /// Как рисовать.
    pub style: Style,
}

/// Кадр: что и откуда рисовать.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Матрица «модель → экран».
    pub view_proj: Mat4,
    /// Направление на свет (от модели к камере, чуть сверху-слева).
    pub light: Vec3,
    /// Куски по порядку.
    pub draws: Vec<Draw>,
    /// Рисовать рёбра.
    pub edges: bool,
}

struct GpuPart {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    edges: Option<(wgpu::Buffer, u32)>,
}

/// Отрисовщик: держит сетки сцены в памяти видеокарты.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: Pipelines,
    parts: Vec<GpuPart>,
    target: Option<Target>,
    stride: u64,
}

/// sRGB (0…255) → линейная яркость для видеокарты.
fn linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_rgba(c: [u8; 4]) -> [f32; 4] {
    [
        linear(c[0]),
        linear(c[1]),
        linear(c[2]),
        c[3] as f32 / 255.0,
    ]
}

/// Цвет рёбер: тёмный, полупрозрачный.
const EDGE_COLOR: [u8; 4] = [40, 44, 52, 150];

impl Gpu {
    /// Отрисовщик на готовом устройстве (от окна или из теста).
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let align = device.limits().min_uniform_buffer_offset_alignment as u64;
        Self {
            pipelines: Pipelines::new(&device),
            stride: DRAW_SIZE.div_ceil(align) * align,
            device,
            queue,
            parts: Vec::new(),
            target: None,
        }
    }

    /// Отрисовщик без окна (тесты, картинки из командной строки).
    /// `None` — подходящей видеокарты нет, даже программной.
    pub fn headless() -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .ok()?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
        Some(Self::new(device, queue))
    }

    /// Загрузить куски сцены, заменив прежние.
    pub fn upload(&mut self, parts: &[Part]) {
        let buffer = |label, contents: &[u8], usage| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage,
                })
        };
        self.parts = parts
            .iter()
            .map(|p| {
                let lines: Vec<Vertex> = p
                    .edges
                    .iter()
                    .map(|&pos| Vertex {
                        pos,
                        normal: [0.0; 3],
                    })
                    .collect();
                GpuPart {
                    vertices: buffer(
                        "vertices",
                        bytemuck::cast_slice(&p.vertices),
                        wgpu::BufferUsages::VERTEX,
                    ),
                    indices: buffer(
                        "indices",
                        bytemuck::cast_slice(&p.indices),
                        wgpu::BufferUsages::INDEX,
                    ),
                    count: p.indices.len() as u32,
                    edges: (!lines.is_empty()).then(|| {
                        let b = buffer(
                            "edges",
                            bytemuck::cast_slice(&lines),
                            wgpu::BufferUsages::VERTEX,
                        );
                        (b, lines.len() as u32)
                    }),
                }
            })
            .collect();
    }

    /// Нарисовать кадр размером `size` пикселей. Возвращает картинку и
    /// признак, что она создана заново (окну нужно перерегистрировать её).
    pub fn render(&mut self, frame: &Frame, size: [u32; 2]) -> (&wgpu::TextureView, bool) {
        let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
        let fresh = self.target.as_ref().is_none_or(|t| t.size != size);
        if fresh {
            self.target = Some(self.make_target(size));
        }

        // Порядок: сплошные, глубина «рентгена», «рентген», рёбра.
        let mut solid = Vec::new();
        let mut ghosts = Vec::new();
        for d in frame.draws.iter().filter(|d| d.part < self.parts.len()) {
            match d.style {
                Style::Solid | Style::Behind => solid.push(*d),
                Style::Ghost => ghosts.push(*d),
            }
        }
        let mut uniforms: Vec<u8> = Vec::new();
        let mut push = |color: [u8; 4], offset: f32, lit: bool| {
            let at = uniforms.len() as u32;
            let mut rec = Vec::with_capacity(self.stride as usize);
            rec.extend_from_slice(bytemuck::cast_slice(&frame.view_proj.to_cols_array()));
            rec.extend_from_slice(bytemuck::cast_slice(&linear_rgba(color)));
            rec.extend_from_slice(bytemuck::cast_slice(&frame.light.extend(0.0).to_array()));
            rec.extend_from_slice(bytemuck::cast_slice(&[offset, lit as u8 as f32, 0.0, 0.0]));
            rec.resize(self.stride as usize, 0);
            uniforms.extend(rec);
            at
        };
        let behind = |s: Style| if s == Style::Solid { 0.0 } else { 2e-4 };
        let solid: Vec<_> = solid
            .iter()
            .map(|d| (d.part, push(d.color, behind(d.style), true)))
            .collect();
        let ghosts: Vec<_> = ghosts
            .iter()
            .map(|d| (d.part, push(d.color, behind(d.style), true)))
            .collect();
        let edge_at = push(EDGE_COLOR, -3e-4, false);
        let edge_parts: Vec<usize> = if frame.edges {
            frame
                .draws
                .iter()
                .map(|d| d.part)
                .filter(|&p| p < self.parts.len())
                .collect()
        } else {
            Vec::new()
        };

        let ubuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("draws"),
                contents: &uniforms,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draws"),
            layout: &self.pipelines.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &ubuf,
                    offset: 0,
                    size: wgpu::BufferSize::new(DRAW_SIZE),
                }),
            }],
        });

        let target = self.target.as_ref().expect("цель создана выше");
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let bg = BACKGROUND.map(|c| linear(c) as f64);
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("nonius view"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.msaa,
                    resolve_target: Some(&target.view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0],
                            g: bg[1],
                            b: bg[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let p = &self.pipelines;
            let mut triangles = |pipeline: &wgpu::RenderPipeline, list: &[(usize, u32)]| {
                pass.set_pipeline(pipeline);
                for &(i, at) in list {
                    let g = &self.parts[i];
                    pass.set_bind_group(0, &bind, &[at]);
                    pass.set_vertex_buffer(0, g.vertices.slice(..));
                    pass.set_index_buffer(g.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..g.count, 0, 0..1);
                }
            };
            triangles(&p.solid, &solid);
            triangles(&p.ghost_depth, &ghosts);
            triangles(&p.ghost, &ghosts);
            pass.set_pipeline(&p.lines);
            pass.set_bind_group(0, &bind, &[edge_at]);
            for i in edge_parts {
                if let Some((buf, n)) = &self.parts[i].edges {
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..*n, 0..1);
                }
            }
        }
        self.queue.submit([enc.finish()]);
        (
            &self.target.as_ref().expect("цель создана выше").shown,
            fresh,
        )
    }
}
