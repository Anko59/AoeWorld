use super::{SceneCamera, WorldLayer, error};
use crate::{
    GAME_ATLAS_SIDE, game_grid,
    surface_mesh::{
        ProjectedSurfaceTriangle, SurfacePoint, surface_render_depth, triangle_texture_coordinates,
    },
};
use wasm_bindgen::Clamped;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

const BACKGROUND: [u8; 4] = [41, 74, 36, 255];
const WATER_TINT: [u8; 3] = [38, 113, 190];

/// Canvas 2D has no depth attachment, so rasterize the same projected
/// triangles as WebGPU into a persistent CPU color/depth buffer. This keeps
/// overlap resolution per pixel without issuing one browser readback per
/// primitive or relying on average painter depth.
pub(super) fn render_canvas_world(
    canvas: &HtmlCanvasElement,
    context: &CanvasRenderingContext2d,
    atlas: &[u8],
    color: &mut Vec<u8>,
    depth: &mut Vec<f64>,
    layers: &[WorldLayer],
    camera: SceneCamera,
    grid: bool,
) -> Result<(), String> {
    let width = canvas.width();
    let height = canvas.height();
    if width == 0 || height == 0 {
        return Ok(());
    }
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4).map(|bytes| (pixels, bytes)))
        .ok_or_else(|| "Canvas depth buffer size overflowed".to_owned())?;
    color.resize(pixel_count.1, 0);
    for pixel in color.chunks_exact_mut(4) {
        pixel.copy_from_slice(&BACKGROUND);
    }
    depth.resize(pixel_count.0, f64::NEG_INFINITY);
    depth.fill(f64::NEG_INFINITY);

    for layer in layers {
        let Some([left, top, right, bottom]) =
            canvas_layer_bounds(layer, camera.viewport, width, height)
        else {
            continue;
        };
        match layer {
            WorldLayer::Surface(triangle) => {
                raster_surface(
                    triangle,
                    atlas,
                    [left, top, right, bottom],
                    width,
                    color,
                    depth,
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
            WorldLayer::Sprite(sprite, frame, layer_depth) => {
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

    let image = ImageData::new_with_u8_clamped_array_and_sh(Clamped(&color[..]), width, height)
        .map_err(error)?;
    context.put_image_data(&image, 0.0, 0.0).map_err(error)?;
    if grid {
        game_grid::draw_grid(context, camera);
    }
    Ok(())
}

fn raster_surface(
    triangle: &ProjectedSurfaceTriangle,
    atlas: &[u8],
    bounds: [u32; 4],
    width: u32,
    color: &mut [u8],
    depth: &mut [f64],
) {
    let texture_uv = triangle.texture_uv;
    let local_uv = triangle_texture_coordinates(triangle.texture_mode);
    let Some(plane) = RasterPlane::new(triangle.points) else {
        return;
    };
    let vertex_depths = triangle
        .points
        .map(|point| surface_render_depth(point.world, triangle.skirt));
    for y in bounds[1]..bounds[3] {
        let screen_y = f64::from(y) + 0.5;
        let mut first =
            plane.x[0] * (f64::from(bounds[0]) + 0.5) + plane.y[0] * screen_y + plane.constant[0];
        let mut second =
            plane.x[1] * (f64::from(bounds[0]) + 0.5) + plane.y[1] * screen_y + plane.constant[1];
        for x in bounds[0]..bounds[2] {
            let third = 1.0 - first - second;
            if first < -1e-7 || second < -1e-7 || third < -1e-7 {
                first += plane.x[0];
                second += plane.x[1];
                continue;
            }
            let weights = [first, second, third];
            let fragment_depth =
                vertex_depths[0] * first + vertex_depths[1] * second + vertex_depths[2] * third;
            let source = if let Some(rect) = texture_uv {
                let local = [0, 1].map(|axis| {
                    local_uv[0][axis] * weights[0]
                        + local_uv[1][axis] * weights[1]
                        + local_uv[2][axis] * weights[2]
                });
                let sample = sample_atlas(
                    atlas,
                    (f64::from(rect[0]) + local[0] * f64::from(rect[2]))
                        * f64::from(GAME_ATLAS_SIDE),
                    (f64::from(rect[1]) + local[1] * f64::from(rect[3]))
                        * f64::from(GAME_ATLAS_SIDE),
                );
                tint_sample(sample, triangle.tint)
            } else {
                [
                    (triangle.color[0] * 255.0).round() as u8,
                    (triangle.color[1] * 255.0).round() as u8,
                    (triangle.color[2] * 255.0).round() as u8,
                    u8::MAX,
                ]
            };
            write_fragment(x, y, width, fragment_depth, source, color, depth);
            first += plane.x[0];
            second += plane.x[1];
        }
    }
}

struct RasterPlane {
    x: [f64; 2],
    y: [f64; 2],
    constant: [f64; 2],
}

impl RasterPlane {
    fn new(points: [SurfacePoint; 3]) -> Option<Self> {
        let [a, b, c] = points.map(|point| point.screen);
        let denominator = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
        if denominator.abs() < f64::EPSILON {
            return None;
        }
        let x = [(b.y - c.y) / denominator, (c.y - a.y) / denominator];
        let y = [(c.x - b.x) / denominator, (a.x - c.x) / denominator];
        let constant = [-x[0] * c.x - y[0] * c.y, -x[1] * c.x - y[1] * c.y];
        Some(Self { x, y, constant })
    }
}

fn raster_sprite(
    sprite: crate::web::Sprite,
    frame: crate::GameFrame,
    layer_depth: f64,
    atlas: &[u8],
    viewport: [f64; 2],
    bounds: [u32; 4],
    width: u32,
    color: &mut [u8],
    depth: &mut [f64],
) {
    let center_x = (f64::from(sprite.position[0]) + 1.0) * viewport[0] * 0.5;
    let center_y = (1.0 - f64::from(sprite.position[1])) * viewport[1] * 0.5;
    let origin_x = center_x - f64::from(frame.size[0]) * 0.5;
    let origin_y = center_y - f64::from(frame.size[1]) * 0.5;
    let [u, v, source_width, source_height] = sprite.uv.map(f64::from);
    for y in bounds[1]..bounds[3] {
        let texture_v =
            v + (f64::from(y) + 0.5 - origin_y) / f64::from(frame.size[1]) * source_height;
        for x in bounds[0]..bounds[2] {
            let horizontal = (f64::from(x) + 0.5 - origin_x) / f64::from(frame.size[0]);
            let texture_u = u + source_width * horizontal;
            let source = sample_atlas(
                atlas,
                texture_u * f64::from(GAME_ATLAS_SIDE),
                texture_v * f64::from(GAME_ATLAS_SIDE),
            );
            write_fragment(x, y, width, layer_depth, source, color, depth);
        }
    }
}

fn raster_selection(
    sprite: crate::web::Sprite,
    layer_depth: f64,
    viewport: [f64; 2],
    bounds: [u32; 4],
    width: u32,
    color: &mut [u8],
    depth: &mut [f64],
) {
    let center_x = (f64::from(sprite.position[0]) + 1.0) * viewport[0] * 0.5;
    let center_y = (1.0 - f64::from(sprite.position[1])) * viewport[1] * 0.5;
    let radius_x = f64::from(sprite.radius[0]) * viewport[0];
    let radius_y = f64::from(sprite.radius[1]) * viewport[1];
    if radius_x <= 0.0 || radius_y <= 0.0 {
        return;
    }
    for y in bounds[1]..bounds[3] {
        for x in bounds[0]..bounds[2] {
            let dx = (f64::from(x) + 0.5 - center_x) / radius_x;
            let dy = (f64::from(y) + 0.5 - center_y) / radius_y;
            if dx * dx + dy * dy <= 1.0 {
                write_fragment(
                    x,
                    y,
                    width,
                    layer_depth,
                    [242, 217, 89, u8::MAX],
                    color,
                    depth,
                );
            }
        }
    }
}

fn sample_atlas(atlas: &[u8], x: f64, y: f64) -> [u8; 4] {
    let side = GAME_ATLAS_SIDE as usize;
    let expected = side * side * 4;
    if atlas.len() != expected {
        return [0; 4];
    }
    let x = x.floor().clamp(0.0, f64::from(GAME_ATLAS_SIDE - 1)) as usize;
    let y = y.floor().clamp(0.0, f64::from(GAME_ATLAS_SIDE - 1)) as usize;
    let start = (y * side + x) * 4;
    [
        atlas[start],
        atlas[start + 1],
        atlas[start + 2],
        atlas[start + 3],
    ]
}

fn tint_sample(mut texel: [u8; 4], tint: u8) -> [u8; 4] {
    match tint {
        1..=3 => {
            let factor = [0.92, 0.78, 0.72][usize::from(tint - 1)];
            for channel in &mut texel[..3] {
                *channel = (f32::from(*channel) * factor).round() as u8;
            }
        }
        4 => {
            for (channel, water) in texel[..3].iter_mut().zip(WATER_TINT) {
                *channel = (f32::from(*channel) * 0.86 + f32::from(water) * 0.14).round() as u8;
            }
        }
        _ => {}
    }
    texel
}

fn write_fragment(
    x: u32,
    y: u32,
    width: u32,
    fragment_depth: f64,
    source: [u8; 4],
    color: &mut [u8],
    depth: &mut [f64],
) {
    if source[3] == 0 || !fragment_depth.is_finite() {
        return;
    }
    let pixel = y as usize * width as usize + x as usize;
    if fragment_depth < depth[pixel] {
        return;
    }
    let offset = pixel * 4;
    if source[3] == u8::MAX {
        color[offset..offset + 4].copy_from_slice(&source);
    } else {
        let source_alpha = f32::from(source[3]) / 255.0;
        let target_alpha = 1.0 - source_alpha;
        for channel in 0..3 {
            color[offset + channel] = (f32::from(source[channel]) * source_alpha
                + f32::from(color[offset + channel]) * target_alpha)
                .round() as u8;
        }
        color[offset + 3] = 255;
    }
    depth[pixel] = fragment_depth;
}

fn canvas_layer_bounds(
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
            let radius_x = f64::from(sprite.radius[0]) * viewport[0];
            let radius_y = f64::from(sprite.radius[1]) * viewport[1];
            (
                center_x - radius_x,
                center_y - radius_y,
                center_x + radius_x,
                center_y + radius_y,
            )
        }
        WorldLayer::Sprite(sprite, frame, _) => {
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
    let left = min_x.floor().clamp(0.0, f64::from(width)) as u32;
    let top = min_y.floor().clamp(0.0, f64::from(height)) as u32;
    let right = max_x.ceil().clamp(0.0, f64::from(width)) as u32;
    let bottom = max_y.ceil().clamp(0.0, f64::from(height)) as u32;
    (right > left && bottom > top).then_some([left, top, right, bottom])
}
