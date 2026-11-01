use crate::surface_mesh::ProjectedSurfaceTriangle;
use aoe_protocol::EntityState;
use bytemuck::{Pod, Zeroable};
use web_sys::HtmlCanvasElement;
use wgpu::SurfaceTarget;
const CAPACITY: usize = 16_384;
const ATLAS_SIDE: u32 = 8;
const ATLAS_BYTES: usize = (ATLAS_SIDE * ATLAS_SIDE * 4) as usize;

#[path = "web_buffer.rs"]
mod instance_buffer;
pub(crate) use instance_buffer::surface_instance;
use instance_buffer::{InstanceBuffer, required_capacity};
#[path = "web/draw.rs"]
mod draw;
#[path = "web/health.rs"]
pub(crate) mod health;
#[path = "web/occlusion.rs"]
mod occlusion;
#[path = "web/presentation.rs"]
mod presentation;
#[path = "web/shaders.rs"]
mod shaders;
#[path = "web/submit.rs"]
mod submit;
#[cfg(test)]
use draw::draw_ranges;

#[cfg(test)]
#[path = "web_tests.rs"]
pub(crate) mod tests;
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
    pub(crate) terrain_blend: [[f32; 4]; 2],
    /// Primary, secondary, tertiary atlas layers. Surface w stores appearance;
    /// Quad w stores sampling/selection tags and occluder/unit-contour bits.
    pub(crate) pages: [u32; 4],
}
impl Sprite {
    pub(crate) const NEAREST_TERRAIN_QUAD: u32 = 1;
    pub(crate) const OCCLUDER: u32 = 4;
    pub(crate) const UNIT_CONTOUR: u32 = 8;

    pub(crate) const PLAYER_MASK: u32 = 64;

    pub(crate) fn with_owner(mut self, visual: crate::SceneUnitVisual) -> Self {
        if let Some(colour) = visual.colour() {
            self.pages[3] |= Self::PLAYER_MASK;
            self.terrain_blend[1][..3].copy_from_slice(&colour.map(|c| f32::from(c) / 255.0));
        }
        self.with_unit_contour(visual.outline_colour())
    }

    pub(crate) fn with_unit_contour(mut self, colour: Option<[u8; 3]>) -> Self {
        if let Some(colour) = colour {
            self.pages[3] |= Self::UNIT_CONTOUR;
            // Object quads leave terrain-blend words free; keep Sprite112 intact.
            self.terrain_blend[0][..3].copy_from_slice(&colour.map(|c| f32::from(c) / 255.0));
        }
        self
    }
}

pub struct Renderer {
    pub(crate) health: health::Health,
    adapter_label: String,
    surface: wgpu::Surface<'static>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub(crate) pipeline: wgpu::RenderPipeline,
    mixed_pipeline: Option<wgpu::RenderPipeline>,
    mask: occlusion::Mask,
    occlusion: Option<occlusion::Contour>,
    has_occlusion: bool,
    draws: Vec<(std::ops::Range<u32>, bool)>,
    contour_draws: Vec<std::ops::Range<u32>>,
    pub(crate) staging: Vec<Sprite>,
    pub(crate) instances: InstanceBuffer,
    pub(crate) _atlas: wgpu::Texture,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    page_retirement: Option<js_sys::Function>,
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(detach) = &self.page_retirement {
            let _ = detach.call0(&wasm_bindgen::JsValue::UNDEFINED);
        }
        // wgpu's browser Device drop is a no-op. Retire the device even when
        // startup health fails or its initialization deadline cancels a future.
        self.shutdown();
    }
}

#[derive(Clone, Copy)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

#[derive(Clone, Copy, Default)]
pub struct Counters {
    /// Distinguishes a submitted clear-only frame from a skipped surface frame.
    pub did_present: bool,
    pub visible: usize,
    pub draw_calls: usize,
    pub gpu_buffer_bytes: usize,
    pub persistent_gpu_resources: usize,
    pub atlas_pages: usize,
    pub atlas_uploads: usize,
    pub atlas_bytes: usize,
}

impl Renderer {
    pub(crate) fn memory_usage(&self) -> (usize, usize) {
        (
            self._atlas.width() as usize
                * self._atlas.height() as usize
                * self._atlas.depth_or_array_layers() as usize
                * 4
                + self.config.width as usize * self.config.height as usize * 5,
            self.instances.bytes(),
        )
    }

    pub async fn new(canvas: HtmlCanvasElement) -> Result<Self, String> {
        shaders::load().await?;
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| format!("WebGPU surface: {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
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
        let health = health::Health::default();
        health.install(&device);
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or("No compatible canvas format")?;
        // Firefox/Wayland PRIME can advertise BGRA while its shared-texture
        // presentation stays black without validation errors. Core RGBA takes
        // Firefox's working presentation path, including on the discrete GPU.
        config.format = wgpu::TextureFormat::Rgba8Unorm;
        surface.configure(&device, &config);

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
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
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
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
        let pipeline = draw::pipeline(&device, &layout, config.format, false);
        let instances = InstanceBuffer::new(
            &device,
            &pipeline.get_bind_group_layout(0),
            &atlas_view,
            &sampler,
        );
        let depth = create_depth_texture(&device, width, height);
        let depth_view = depth.create_view(&Default::default());
        let mask = occlusion::Mask::new(&device, width, height);
        // Rust clients can outlive their document through callback cycles.
        // Release native GPU work on navigation, before browser context teardown.
        let page_retirement = device
            .as_webgpu()
            .map(|device| presentation::retire_with_page(device.as_ref()));
        web_sys::console::info_1(
            &format!(
                "WebGPU adapter {adapter_label}; canvas {:?}; core features/default limits",
                config.format
            )
            .into(),
        );
        let mut renderer = Self {
            health,
            adapter_label,
            surface,
            device,
            queue,
            config,
            pipeline,
            mixed_pipeline: None,
            mask,
            occlusion: None,
            has_occlusion: false,
            draws: Vec::new(),
            contour_draws: Vec::new(),
            staging: Vec::new(),
            instances,
            _atlas: atlas,
            depth,
            depth_view,
            page_retirement,
        };
        // Verify a known coloured clear before assets, fog or networking can
        // legitimately leave the game black. Queue success alone is insufficient.
        renderer.render_sprites_with_clear(&[], presentation::CLEAR)?;
        if let Err(error) = health::within(presentation::check(canvas)).await {
            renderer.shutdown();
            return Err(error);
        }
        renderer.check_health()?;
        Ok(renderer)
    }

    pub fn adapter_label(&self) -> &str {
        &self.adapter_label
    }

    pub fn check_health(&self) -> Result<(), String> {
        self.health.check()
    }
    pub(crate) fn atlas_updates_ready(&self) -> Result<bool, String> {
        self.health.uploads_ready()
    }

    pub fn page_retired(&self) -> bool {
        presentation::page_retired()
    }

    pub fn shutdown(&self) {
        self.device.destroy();
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || (self.config.width == width && self.config.height == height)
        {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth.destroy();
        self.depth = create_depth_texture(&self.device, width, height);
        self.depth_view = self.depth.create_view(&Default::default());
        self.mask.texture.destroy();
        self.mask = occlusion::Mask::new(&self.device, width, height);
        if let Some(contour) = &mut self.occlusion {
            contour.rebind(&self.device, &self.mask.view);
        }
    }

    pub fn render(
        &mut self,
        entities: impl Iterator<Item = EntityState>,
        camera: Camera,
    ) -> Result<Counters, String> {
        let width = self.config.width as f32;
        let height = self.config.height as f32;
        let mut sprites = std::mem::take(&mut self.staging);
        sprites.clear();
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
                terrain_blend: [[0.0; 4]; 2],
                pages: [0; 4],
            });
        }
        let result = self.render_instances(&mut sprites, [0.055, 0.08, 0.12, 1.0]);
        self.staging = sprites;
        result
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

    pub(crate) fn draw_on(&self, pass: &mut wgpu::RenderPass<'_>, count: u32) {
        self.instances.set_on(pass);
        for (range, mixed) in &self.draws {
            assert!(range.end <= count, "draw exceeds instances");
            let pipeline = if *mixed {
                self.mixed_pipeline.as_ref().expect("mixed pipeline")
            } else {
                &self.pipeline
            };
            pass.set_pipeline(pipeline);
            pass.draw(0..6, range.clone());
        }
    }
}

pub(crate) fn normalize_depths(instances: &mut [Sprite]) -> bool {
    let (mut units, mut occluders) = (false, false);
    let mut range: Option<(f32, f32)> = None;
    for sprite in instances.iter() {
        if sprite.color[3] >= 0.0 {
            // H || (U && O) == (U || H) && (O || H): retain the existing
            // two flag checks, including for units, with no hover-only scan.
            units |= sprite.pages[3] & (Sprite::UNIT_CONTOUR | 48) != 0;
            occluders |= sprite.pages[3] & (Sprite::OCCLUDER | 48) != 0;
        }
        for depth in sprite.depths[..3].iter().copied() {
            if depth.is_finite() {
                range = Some(match range {
                    Some((minimum, maximum)) => (minimum.min(depth), maximum.max(depth)),
                    None => (depth, depth),
                });
            }
        }
    }
    let (minimum, maximum) = range.unwrap_or((0.0, 0.0));
    let span = maximum - minimum;
    for sprite in instances {
        // Variant-tagged surface w carries an exact numeric 24-bit floor packet,
        // not geometry depth. Manual, metadata-free and object packets keep depth normalization.
        let count = if sprite.color[3] < 0.0
            && sprite.pages[3] & (crate::surface_mesh::landscape::INTERPOLATED_FLOOR | 1)
                == (crate::surface_mesh::landscape::INTERPOLATED_FLOOR | 1)
        {
            3
        } else {
            4
        };
        for depth in &mut sprite.depths[..count] {
            *depth = if *depth == f32::NEG_INFINITY {
                1.0
            } else if !depth.is_finite() {
                0.0
            } else if span <= f32::EPSILON {
                0.5
            } else {
                ((maximum - *depth) / span).clamp(0.0, 1.0)
            };
        }
    }
    units && occluders
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
