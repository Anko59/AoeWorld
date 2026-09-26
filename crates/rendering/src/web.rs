use crate::surface_mesh::ProjectedSurfaceTriangle;
use aoe_protocol::EntityState;
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use web_sys::HtmlCanvasElement;
use wgpu::SurfaceTarget;
const CAPACITY: usize = 16_384;
const ATLAS_SIDE: u32 = 8;
const ATLAS_BYTES: usize = (ATLAS_SIDE * ATLAS_SIDE * 4) as usize;

#[path = "web_buffer.rs"]
mod instance_buffer;
use instance_buffer::{InstanceBuffer, required_capacity};

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Sprite {
    pub(crate) position: [f32; 2],
    pub(crate) radius: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) uv: [f32; 4],
    /// Camera-relative world depth for the sprite's vertices. Terrain uses
    /// three values so the depth buffer interpolates the actual surface plane.
    pub(crate) depths: [f32; 4],
}
pub struct Renderer {
    adapter_label: String,
    surface: wgpu::Surface<'static>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) instances: InstanceBuffer,
    pub(crate) _atlas: wgpu::Texture,
    depth: wgpu::Texture,
}

#[derive(Clone, Copy)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

#[derive(Clone, Copy, Default)]
pub struct Counters {
    pub visible: usize,
    pub draw_calls: usize,
    pub gpu_buffer_bytes: usize,
    pub persistent_gpu_resources: usize,
    pub atlas_pages: usize,
    pub atlas_uploads: usize,
    pub atlas_bytes: usize,
}

impl Renderer {
    pub async fn new(canvas: HtmlCanvasElement) -> Result<Self, String> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| format!("WebGPU surface: {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("No WebGPU adapter: {e}"))?;
        let info = adapter.get_info();
        let adapter_label = format!("{:?}: {}", info.backend, info.name);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| format!("WebGPU device: {e}"))?;
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or("No compatible canvas format")?;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("synthetic sprites"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("sprites.wgsl"))),
        });
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("synthetic sprite atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIDE,
                height: ATLAS_SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut pixels = [0_u8; ATLAS_BYTES];
        for y in 0..ATLAS_SIDE {
            for x in 0..ATLAS_SIDE {
                let pixel = &mut pixels[((y * ATLAS_SIDE + x) * 4) as usize..][..4];
                pixel.copy_from_slice(&[
                    255,
                    255,
                    255,
                    if (1..7).contains(&x) && (1..7).contains(&y) {
                        255
                    } else {
                        0
                    },
                ]);
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_SIDE * 4),
                rows_per_image: Some(ATLAS_SIDE),
            },
            wgpu::Extent3d {
                width: ATLAS_SIDE,
                height: ATLAS_SIDE,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("synthetic sprite sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprites"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let instances = InstanceBuffer::new(
            &device,
            &pipeline.get_bind_group_layout(0),
            &atlas_view,
            &sampler,
        );
        let depth = create_depth_texture(&device, width, height);
        Ok(Self {
            adapter_label,
            surface,
            device,
            queue,
            config,
            pipeline,
            instances,
            _atlas: atlas,
            depth,
        })
    }

    pub fn adapter_label(&self) -> &str {
        &self.adapter_label
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || (self.config.width == width && self.config.height == height)
        {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth_texture(&self.device, width, height);
    }

    pub fn render(
        &mut self,
        entities: impl Iterator<Item = EntityState>,
        camera: Camera,
    ) -> Result<Counters, String> {
        let width = self.config.width as f32;
        let height = self.config.height as f32;
        let mut sprites = Vec::new();
        for entity in entities {
            let x = (entity.position.x as f32 - camera.x) * camera.zoom;
            let y = (entity.position.y as f32 - camera.y) * camera.zoom;
            if x < -8.0 || y < -8.0 || x > width + 8.0 || y > height + 8.0 {
                continue;
            }
            if sprites.len() == CAPACITY {
                break;
            }
            let palette = [
                [0.32, 0.73, 0.95, 1.0],
                [0.97, 0.53, 0.36, 1.0],
                [0.53, 0.89, 0.54, 1.0],
                [0.96, 0.77, 0.32, 1.0],
            ];
            sprites.push(Sprite {
                position: [x / width * 2.0 - 1.0, 1.0 - y / height * 2.0],
                radius: [3.0 * camera.zoom / width, 3.0 * camera.zoom / height],
                color: palette[entity.player.0 as usize % palette.len()],
                uv: [0.0, 0.0, 1.0, 1.0],
                depths: [0.0; 4],
            });
        }
        self.render_sprites(&sprites)
    }

    pub(crate) fn render_sprites(&mut self, sprites: &[Sprite]) -> Result<Counters, String> {
        self.render_sprites_with_clear(sprites, [0.055, 0.08, 0.12, 1.0])
    }

    pub(crate) fn render_sprites_with_clear(
        &mut self,
        sprites: &[Sprite],
        clear: [f64; 4],
    ) -> Result<Counters, String> {
        self.render_world_layers(&[], sprites, clear)
    }

    pub(crate) fn render_world_layers(
        &mut self,
        surfaces: &[ProjectedSurfaceTriangle],
        sprites: &[Sprite],
        clear: [f64; 4],
    ) -> Result<Counters, String> {
        let required = required_capacity(surfaces, sprites)?;
        self.instances.ensure_capacity(
            required,
            &self.device,
            &self.pipeline.get_bind_group_layout(0),
        )?;
        let mut instances = Vec::with_capacity(required);
        let width = self.config.width.max(1) as f64;
        let height = self.config.height.max(1) as f64;
        for triangle in surfaces {
            instances.push(surface_instance(triangle, [width, height], 0.0));
        }
        instances.extend_from_slice(sprites);
        normalize_depths(&mut instances);
        self.instances.write(&self.queue, &instances);
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(Counters {
                    visible: sprites.len(),
                    draw_calls: 0,
                    gpu_buffer_bytes: self.instances.bytes(),
                    persistent_gpu_resources: 7,
                    atlas_pages: 1,
                    atlas_uploads: 1,
                    atlas_bytes: ATLAS_BYTES,
                });
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(Counters {
                    visible: sprites.len(),
                    draw_calls: 0,
                    gpu_buffer_bytes: self.instances.bytes(),
                    persistent_gpu_resources: 7,
                    atlas_pages: 1,
                    atlas_uploads: 1,
                    atlas_bytes: ATLAS_BYTES,
                });
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return Err("WebGPU surface lost; reload to restore it".to_owned());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("WebGPU surface validation failed".to_owned());
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = self
            .depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sprites"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world layers"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear[0],
                            g: clear[1],
                            b: clear[2],
                            a: clear[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            self.instances.set_on(&mut pass);
            pass.draw(0..6, 0..instances.len() as u32);
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        Ok(Counters {
            visible: sprites.len(),
            draw_calls: usize::from(!instances.is_empty()),
            gpu_buffer_bytes: self.instances.bytes(),
            persistent_gpu_resources: 7,
            atlas_pages: 1,
            atlas_uploads: 1,
            atlas_bytes: ATLAS_BYTES,
        })
    }
}

fn screen_to_clip(x: f64, y: f64, width: f64, height: f64) -> [f32; 2] {
    [
        (x / width * 2.0 - 1.0) as f32,
        (1.0 - y / height * 2.0) as f32,
    ]
}

pub(crate) fn surface_instance(
    triangle: &ProjectedSurfaceTriangle,
    viewport: [f64; 2],
    depth_origin: f64,
) -> Sprite {
    let points = triangle.points.map(|point| {
        screen_to_clip(
            point.screen.x,
            point.screen.y,
            viewport[0].max(1.0),
            viewport[1].max(1.0),
        )
    });
    let depths = triangle.points.map(|point| {
        (crate::surface_mesh::surface_render_depth(point.world, triangle.skirt) - depth_origin)
            as f32
    });
    let second = points[1];
    let third = points[2];
    match triangle.texture_uv {
        Some(uv) => Sprite {
            position: points[0],
            radius: second,
            color: [
                third[0],
                third[1],
                f32::from(triangle.tint) * 8.0 + f32::from(triangle.texture_mode),
                -1.0,
            ],
            uv,
            depths: [depths[0], depths[1], depths[2], 0.0],
        },
        None => Sprite {
            position: points[0],
            radius: second,
            color: [third[0], third[1], 0.0, -2.0],
            uv: [triangle.color[0], triangle.color[1], triangle.color[2], 1.0],
            depths: [depths[0], depths[1], depths[2], 0.0],
        },
    }
}

fn normalize_depths(instances: &mut [Sprite]) {
    let Some((minimum, maximum)) = instances
        .iter()
        .flat_map(|sprite| sprite.depths[..3].iter().copied())
        .filter(|depth| depth.is_finite())
        .fold(None, |range: Option<(f32, f32)>, depth| {
            Some(range.map_or((depth, depth), |(minimum, maximum)| {
                (minimum.min(depth), maximum.max(depth))
            }))
        })
    else {
        for sprite in instances {
            sprite.depths = [0.0; 4];
        }
        return;
    };
    let span = maximum - minimum;
    for sprite in instances {
        for depth in &mut sprite.depths {
            *depth = if !depth.is_finite() {
                0.0
            } else if span <= f32::EPSILON {
                0.5
            } else {
                ((maximum - *depth) / span).clamp(0.0, 1.0)
            };
        }
    }
}

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("world depth"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24Plus,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
