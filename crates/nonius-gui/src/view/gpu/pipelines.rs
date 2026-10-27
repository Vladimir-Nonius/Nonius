//! Конвейеры видеокарты: сплошная деталь, «рентген» и рёбра.

use super::{COLOR_FORMAT, DEPTH_FORMAT, SAMPLES};
use crate::view::mesh::Vertex;

/// Все конвейеры и общая раскладка данных.
pub struct Pipelines {
    /// Раскладка: одна равномерная структура `Draw` со сдвигом.
    pub layout: wgpu::BindGroupLayout,
    /// Сплошная поверхность.
    pub solid: wgpu::RenderPipeline,
    /// «Рентген», шаг 1: только глубина ближайшей поверхности.
    pub ghost_depth: wgpu::RenderPipeline,
    /// «Рентген», шаг 2: полупрозрачный цвет ближайшей поверхности.
    pub ghost: wgpu::RenderPipeline,
    /// Рёбра.
    pub lines: wgpu::RenderPipeline,
}

/// Размер одной записи `Draw` в шейдере, байт.
pub const DRAW_SIZE: u64 = 112;

impl Pipelines {
    /// Собрать конвейеры.
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nonius shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nonius draw"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(DRAW_SIZE),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nonius"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let make = |label: &str, topology, depth_write, compare, blend, colors| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    }],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: depth_write,
                    depth_compare: compare,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend,
                        write_mask: colors,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        use wgpu::{BlendState, ColorWrites, CompareFunction as Cmp, PrimitiveTopology as Topo};
        let alpha = Some(BlendState::ALPHA_BLENDING);
        Self {
            solid: make(
                "solid",
                Topo::TriangleList,
                true,
                Cmp::Less,
                None,
                ColorWrites::ALL,
            ),
            ghost_depth: make(
                "ghost depth",
                Topo::TriangleList,
                true,
                Cmp::Less,
                None,
                ColorWrites::empty(),
            ),
            ghost: make(
                "ghost",
                Topo::TriangleList,
                false,
                Cmp::LessEqual,
                alpha,
                ColorWrites::ALL,
            ),
            lines: make(
                "lines",
                Topo::LineList,
                false,
                Cmp::LessEqual,
                alpha,
                ColorWrites::ALL,
            ),
            layout,
        }
    }
}
