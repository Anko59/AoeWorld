//! Source specialization keeps the ground mixer off ordinary GPU primitives.
//! Contiguous runs preserve painter order, depth ties and transparent coverage.
use super::Sprite;
use std::{borrow::Cow, ops::Range};

pub(super) fn draw_ranges(instances: &[Sprite]) -> Vec<(Range<u32>, bool)> {
    let mut ranges: Vec<(Range<u32>, bool)> = Vec::new();
    for (index, sprite) in instances.iter().enumerate() {
        let mixed = sprite.color[3] == -3.0
            && sprite.color[2] as u32 % 8 == 6
            && sprite.pages[3] & 1 != 0
            && sprite.terrain_blend[0][2] as u32 != 0;
        if let Some((range, prior)) = ranges.last_mut() {
            if *prior == mixed {
                range.end = index as u32 + 1;
                continue;
            }
        }
        ranges.push((index as u32..index as u32 + 1, mixed));
    }
    ranges
}

pub(super) fn shader_source(mixed: bool) -> &'static str {
    super::shaders::source(mixed)
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    mixed: bool,
) -> wgpu::RenderPipeline {
    pipeline_for(device, layout, format, mixed, None)
}

pub(super) fn contour_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    mask: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    pipeline_for(device, layout, format, false, Some(mask))
}

fn pipeline_for(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    mixed: bool,
    mask: Option<&wgpu::BindGroupLayout>,
) -> wgpu::RenderPipeline {
    let source = Cow::Borrowed(shader_source(mixed));
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shared world shader"),
        source: wgpu::ShaderSource::Wgsl(source),
    });
    let mut layouts = vec![Some(layout)];
    layouts.extend(mask.map(Some));
    let mut targets = vec![Some(wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    if mask.is_none() {
        targets.push(Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::R8Uint,
            blend: None,
            write_mask: wgpu::ColorWrites::RED,
        }));
    }
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sprite pipeline layout"),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sprite pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some(if mask.is_some() {
                "vs_unit_contour"
            } else {
                "vs_main"
            }),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(if mask.is_some() {
                "fs_unit_contour"
            } else {
                "fs_main"
            }),
            targets: &targets,
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: Some(mask.is_none()),
            depth_compare: Some(if mask.is_some() {
                wgpu::CompareFunction::Always
            } else {
                wgpu::CompareFunction::LessEqual
            }),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
