//! Frozen pre-optimization pixel oracle; test-only.
use super::*;

pub(super) fn raster_surface(
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
                let mut sample = sample_terrain_atlas(atlas, rect, local);
                if let Some([second, third]) = triangle.texture_blend {
                    let samples = [
                        sample,
                        sample_terrain_atlas(atlas, second, local),
                        sample_terrain_atlas(atlas, third, local),
                    ];
                    sample = std::array::from_fn(|channel| {
                        (f64::from(samples[0][channel]) * first
                            + f64::from(samples[1][channel]) * weights[1]
                            + f64::from(samples[2][channel]) * weights[2])
                            .round() as u8
                    });
                }
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

fn sample_terrain_atlas(atlas: &[u8], address: crate::AtlasAddress, local: [f64; 2]) -> [u8; 4] {
    let rect = address.uv;
    let side = f64::from(GAME_ATLAS_SIDE);
    let point = [0, 1].map(|axis| {
        f64::from(rect[axis]) * side
            + 0.5
            + local[axis] * (f64::from(rect[axis + 2]) * side - 1.0).max(0.0)
    });
    sample_atlas(atlas, address.page, point[0], point[1])
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

pub(super) fn raster_sprite(
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
            let mut source = sample_atlas(
                atlas,
                sprite.pages[0],
                texture_u * f64::from(GAME_ATLAS_SIDE),
                texture_v * f64::from(GAME_ATLAS_SIDE),
            );
            for channel in 0..4 {
                source[channel] = (f32::from(source[channel]) * sprite.color[channel])
                    .clamp(0.0, 255.0)
                    .round() as u8;
            }
            write_fragment(x, y, width, layer_depth, source, color, depth);
        }
    }
}

pub(super) fn sample_atlas(atlas: &[u8], page: u32, x: f64, y: f64) -> [u8; 4] {
    let side = GAME_ATLAS_SIDE as usize;
    let expected = crate::GAME_ATLAS_BYTES;
    if atlas.len() != expected || page >= crate::GAME_ATLAS_PAGES {
        return [0; 4];
    }
    let x = x.floor().clamp(0.0, f64::from(GAME_ATLAS_SIDE - 1)) as usize;
    let y = y.floor().clamp(0.0, f64::from(GAME_ATLAS_SIDE - 1)) as usize;
    let start = page as usize * crate::GAME_ATLAS_PAGE_BYTES + (y * side + x) * 4;
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
    // Negative infinity is the explicit flat-background layer, which must
    // paint into an empty buffer while remaining behind every world object.
    if source[3] == 0 || fragment_depth.is_nan() || fragment_depth == f64::INFINITY {
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
