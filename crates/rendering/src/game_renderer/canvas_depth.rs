use super::{SceneCamera, WorldLayer, error};
use crate::game_grid;
use js_sys::Uint8ClampedArray;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

const BACKGROUND: [u8; 4] = [41, 74, 36, 255];
#[path = "canvas_raster.rs"]
mod raster;
use raster::{raster_selection, raster_sprite};
#[path = "canvas_depth/damage.rs"]
mod damage;
#[path = "canvas_depth/equality.rs"]
mod equality;

/// Reuses both the WASM-side raster buffers and the browser-owned ImageData
/// backing array. A resize only wraps a new ImageData view around the retained
/// JS array while it stays within the WASM buffers' shrink policy, so camera
/// and window resizes do not leave full-frame garbage for the JS collector.
pub struct CanvasPresentation {
    pub(super) color_buffer: Vec<u8>,
    pub(super) depth_buffer: Vec<f64>,
    pub(super) image_data: Option<ImageData>,
    pub(super) image_pixels: Option<Uint8ClampedArray>,
    pub(super) image_store: Option<Uint8ClampedArray>,
    damage: damage::Damage,
    size: [u32; 2],
}

impl CanvasPresentation {
    pub(super) fn resident_bytes(&self) -> usize {
        self.color_buffer.capacity()
            + self.depth_buffer.capacity() * std::mem::size_of::<f64>()
            + self
                .image_store
                .as_ref()
                .map_or(0, |pixels| pixels.length() as usize)
            + self.damage.resident_bytes()
    }

    pub(super) fn new(width: u32, height: u32) -> Self {
        Self {
            color_buffer: Vec::new(),
            depth_buffer: Vec::new(),
            image_data: None,
            image_pixels: None,
            image_store: None,
            damage: Default::default(),
            size: [width, height],
        }
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) {
        if self.size == [width, height] {
            return;
        }
        self.size = [width, height];
        self.invalidate();
        self.image_data = None;
        self.image_pixels = None;

        if let Some((pixels, color_bytes)) = buffer_lengths(width, height) {
            shrink_after_large_resize(&mut self.color_buffer, color_bytes);
            shrink_after_large_resize(&mut self.depth_buffer, pixels);
        }
    }

    pub(super) fn invalidate(&mut self) {
        self.damage.invalidate();
    }

    fn present(
        &mut self,
        context: &CanvasRenderingContext2d,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        if self.image_data.is_none() || self.size != [width, height] {
            let byte_len = u32::try_from(self.color_buffer.len())
                .map_err(|_| "Canvas image buffer exceeds the typed-array limit".to_owned())?;
            let pixels = self.image_view(byte_len);
            let image = ImageData::new_with_js_u8_clamped_array_and_sh(&pixels, width, height)
                .map_err(error)?;
            self.image_pixels = Some(pixels);
            self.image_data = Some(image);
            self.size = [width, height];
        }
        let Some(pixels) = &self.image_pixels else {
            return Err("Canvas ImageData buffer is unavailable".to_owned());
        };
        pixels.copy_from(&self.color_buffer);
        let Some(image) = &self.image_data else {
            return Err("Canvas ImageData is unavailable".to_owned());
        };
        context.put_image_data(image, 0.0, 0.0).map_err(error)
    }

    /// Returns a `byte_len` view of the retained JS array, reallocating only
    /// when it is too small or more than twice the needed size.
    fn image_view(&mut self, byte_len: u32) -> Uint8ClampedArray {
        let store = match self.image_store.take() {
            Some(store)
                if store.length() >= byte_len && byte_len.saturating_mul(2) >= store.length() =>
            {
                store
            }
            _ => Uint8ClampedArray::new_with_length(byte_len),
        };
        let view = if store.length() == byte_len {
            store.clone()
        } else {
            store.subarray(0, byte_len)
        };
        self.image_store = Some(store);
        view
    }
}

impl Default for CanvasPresentation {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

fn buffer_lengths(width: u32, height: u32) -> Option<(usize, usize)> {
    let pixels = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?;
    Some((pixels, pixels.checked_mul(4)?))
}

fn shrink_after_large_resize<T>(buffer: &mut Vec<T>, target_len: usize) {
    if target_len.saturating_mul(2) < buffer.capacity() {
        buffer.truncate(target_len);
        buffer.shrink_to_fit();
    }
}

/// Canvas 2D has no depth attachment, so rasterize the same projected
/// triangles as WebGPU into a persistent CPU color/depth buffer. This keeps
/// overlap resolution per pixel without issuing one browser readback per
/// primitive or relying on average painter depth.
#[cfg(test)]
pub(super) fn render_canvas_world(
    canvas: &HtmlCanvasElement,
    context: &CanvasRenderingContext2d,
    atlas: &[u8],
    presentation: &mut CanvasPresentation,
    layers: &[WorldLayer],
    camera: SceneCamera,
    grid: bool,
) -> Result<(), String> {
    render_canvas_world_with_fog(
        canvas,
        context,
        atlas,
        presentation,
        layers,
        camera,
        grid,
        false,
        &|_| 1.0,
    )
}

pub(super) fn render_canvas_world_with_fog(
    canvas: &HtmlCanvasElement,
    context: &CanvasRenderingContext2d,
    atlas: &[u8],
    presentation: &mut CanvasPresentation,
    layers: &[WorldLayer],
    camera: SceneCamera,
    grid: bool,
    fog_active: bool,
    light: &dyn Fn([f64; 2]) -> f32,
) -> Result<(), String> {
    let width = canvas.width();
    let height = canvas.height();
    if width == 0 || height == 0 {
        return Ok(());
    }
    let (pixel_count, color_bytes) = buffer_lengths(width, height)
        .ok_or_else(|| "Canvas depth buffer size overflowed".to_owned())?;
    presentation.resize(width, height);
    let lights = layers
        .iter()
        .map(|layer| match layer {
            WorldLayer::Surface(triangle) => super::fog::vertices(triangle, light),
            _ => [1.0; 3],
        })
        .collect::<Vec<_>>();
    let damage = presentation.damage.regions(
        layers,
        &lights,
        camera.viewport,
        [width, height],
        fog_active,
        atlas,
    );
    let color = &mut presentation.color_buffer;
    let depth = &mut presentation.depth_buffer;
    color.resize(color_bytes, 0);
    depth.resize(pixel_count, f64::NEG_INFINITY);
    for &[dirty_left, dirty_top, dirty_right, dirty_bottom] in &damage {
        for y in dirty_top..dirty_bottom {
            let start = (y * width + dirty_left) as usize;
            let end = (y * width + dirty_right) as usize;
            for pixel in color[start * 4..end * 4].as_chunks_mut::<4>().0 {
                pixel.copy_from_slice(if fog_active {
                    &[0, 0, 0, 255]
                } else {
                    &BACKGROUND
                });
            }
            depth[start..end].fill(f64::NEG_INFINITY);
        }

        for (layer, fog) in layers.iter().zip(&lights) {
            let Some([original_left, top, right, bottom]) =
                canvas_layer_bounds(layer, camera.viewport, width, height)
            else {
                continue;
            };
            let left = original_left.max(dirty_left);
            let top = top.max(dirty_top);
            let right = right.min(dirty_right);
            let bottom = bottom.min(dirty_bottom);
            if left >= right || top >= bottom {
                continue;
            }
            match layer {
                WorldLayer::Surface(triangle) => {
                    raster::raster_surface_region(
                        triangle,
                        atlas,
                        [original_left, top, right, bottom],
                        left,
                        width,
                        color,
                        depth,
                        *fog,
                    );
                }
                WorldLayer::Selection(sprite, layer_depth) => {
                    raster_selection(
                        *sprite,
                        *layer_depth,
                        camera.viewport,
                        [left, top, right, bottom],
                        width,
                        color,
                        depth,
                    );
                }
                WorldLayer::Sprite(sprite, frame, layer_depth, _) => {
                    raster_sprite(
                        *sprite,
                        *frame,
                        *layer_depth,
                        atlas,
                        camera.viewport,
                        [left, top, right, bottom],
                        width,
                        color,
                        depth,
                    );
                }
            }
        }
    }

    presentation.present(context, width, height)?;
    presentation.damage.remember(
        layers,
        &lights,
        camera.viewport,
        [width, height],
        fog_active,
        atlas,
    );
    if grid {
        game_grid::draw_grid(context, &super::fog::grid_segments(layers, camera, light));
    }
    Ok(())
}

pub(super) fn canvas_layer_bounds(
    layer: &WorldLayer,
    viewport: [f64; 2],
    width: u32,
    height: u32,
) -> Option<[u32; 4]> {
    let (min_x, min_y, max_x, max_y) = match layer {
        WorldLayer::Surface(triangle) => {
            let bounds = triangle.points.iter().fold(
                [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ],
                |mut bounds, point| {
                    bounds[0] = bounds[0].min(point.screen.x);
                    bounds[1] = bounds[1].min(point.screen.y);
                    bounds[2] = bounds[2].max(point.screen.x);
                    bounds[3] = bounds[3].max(point.screen.y);
                    bounds
                },
            );
            (bounds[0], bounds[1], bounds[2], bounds[3])
        }
        WorldLayer::Selection(sprite, _) => {
            let center_x = (f64::from(sprite.position[0]) + 1.0) * viewport[0] * 0.5;
            let center_y = (1.0 - f64::from(sprite.position[1])) * viewport[1] * 0.5;
            // Quad radii are clip-space half extents (twice the pixel radius).
            let radius_x = f64::from(sprite.radius[0]) * viewport[0] * 0.5;
            let radius_y = f64::from(sprite.radius[1]) * viewport[1] * 0.5;
            (
                center_x - radius_x,
                center_y - radius_y,
                center_x + radius_x,
                center_y + radius_y,
            )
        }
        WorldLayer::Sprite(sprite, frame, _, _) => {
            let center_x = (f64::from(sprite.position[0]) + 1.0) * viewport[0] * 0.5;
            let center_y = (1.0 - f64::from(sprite.position[1])) * viewport[1] * 0.5;
            let half_width = f64::from(frame.size[0]) * 0.5;
            let half_height = f64::from(frame.size[1]) * 0.5;
            (
                center_x - half_width,
                center_y - half_height,
                center_x + half_width,
                center_y + half_height,
            )
        }
    };
    let left = aoe_core::clamp_f64(min_x.floor(), 0.0, f64::from(width)) as u32;
    let top = aoe_core::clamp_f64(min_y.floor(), 0.0, f64::from(height)) as u32;
    let right = aoe_core::clamp_f64(max_x.ceil(), 0.0, f64::from(width)) as u32;
    let bottom = aoe_core::clamp_f64(max_y.ceil(), 0.0, f64::from(height)) as u32;
    (right > left && bottom > top).then_some([left, top, right, bottom])
}
