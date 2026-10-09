use crate::surface_mesh::ProjectedSurfaceTriangle;
use aoe_protocol::EntityState;
use bytemuck::{Pod, Zeroable};
use web_sys::HtmlCanvasElement;
// Generated at build time from the readable WGSL; no runtime decompression.
include!(concat!(env!("OUT_DIR"), "/sprites_shader.rs"));
const CAPACITY: usize = 16_384;
const ATLAS_SIDE: u32 = 8;

pub(crate) mod gpu_bridge;
#[path = "web_buffer.rs"]
mod instance_buffer;
mod submission;
use gpu_bridge::{GpuBridge, error};
use instance_buffer::{InstanceBuffer, required_capacity};
pub(crate) use instance_buffer::{retain_world_packet, surface_instance};
use wasm_bindgen::JsCast;

#[cfg(test)]
#[path = "web/tests/bridge.rs"]
mod bridge_tests;
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
    pub(crate) terrain_blend: [[f32; 4]; 2],
    /// Primary, secondary, tertiary atlas layers, then appearance (legacy zero).
    pub(crate) pages: [u32; 4],
}
pub struct Renderer {
    adapter_label: String,
    pub(crate) device: GpuBridge,
    config: Configuration,
    pub(crate) instances: InstanceBuffer,
    pub(crate) world_atlas: Option<u32>,
    pub(crate) atlas_side: u32,
    pub(crate) atlas_pages: u32,
    resize_error: Option<String>,
}

struct Configuration {
    width: u32,
    height: u32,
    #[cfg(test)]
    format: String,
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
    pub async fn new(canvas: HtmlCanvasElement) -> Result<Self, String> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let promise = gpu_bridge::create(
            &canvas,
            SHADER_SOURCE,
            instance_buffer::INITIAL_CAPACITY as u32,
        )
        .map_err(error)?;
        let device: GpuBridge = wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(error)?
            .unchecked_into();
        let adapter_label = device.adapter_label();
        Ok(Self {
            adapter_label,
            config: Configuration {
                width,
                height,
                #[cfg(test)]
                format: device.format(),
            },
            device,
            instances: InstanceBuffer::new(),
            world_atlas: None,
            atlas_side: ATLAS_SIDE,
            atlas_pages: 1,
            resize_error: None,
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
        self.resize_error = self.device.resize(width, height).map_err(error).err();
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
                terrain_blend: [[0.0; 4]; 2],
                pages: [0; 4],
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
        self.reserve_packets(required)?;
        let mut instances = Vec::with_capacity(required);
        let width = self.config.width.max(1) as f64;
        let height = self.config.height.max(1) as f64;
        for triangle in surfaces {
            instances.push(surface_instance(triangle, [width, height], 0.0));
        }
        instances.extend_from_slice(sprites);
        self.submit_packets(&mut instances, sprites.len(), clear)
    }
}

pub(crate) fn normalize_depths(instances: &mut [Sprite]) {
    let mut range: Option<(f32, f32)> = None;
    for sprite in instances.iter() {
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
        // not geometry depth. Legacy/manual/object packets keep old normalization.
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
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.device.dispose();
    }
}
