use crate::{
    GAME_ATLAS_SIDE,
    surface_mesh::{
        ProjectedSurfaceTriangle, SurfacePoint, surface_render_depth, triangle_texture_coordinates,
    },
};
const WATER_TINT: [u8; 3] = [38, 113, 190];

pub(super) fn raster_surface(
    triangle: &ProjectedSurfaceTriangle,
    atlas: &[u8],
    bounds: [u32; 4],
    width: u32,
    color: &mut [u8],
    depth: &mut [f64],
) {
    let texture = triangle.texture_uv;
    if texture.is_some() && !valid_atlas(atlas) {
        return; // Every sampled alpha would be zero, including water/blend tint.
    }
    let blend = triangle.texture_blend;
    let appearance = triangle.appearance;
    let landscape = appearance != 0
        && triangle.tint <= 3
        && matches!(triangle.material, 0 | 1 | 2 | 6)
        && !triangle.skirt;
    // Canonical endpoints are already quantized; normalize once per Canvas surface.
    let floor_endpoints = triangle
        .floor_strengths
        .map(|values| values.map(|value| f32::from(value) / 255.0));
    let local_uv = triangle_texture_coordinates(triangle.texture_mode);
    let Some(plane) = RasterPlane::new(triangle.points) else {
        return;
    };
    let mut vertex_depths = [0.0; 3];
    for index in 0..3 {
        vertex_depths[index] = surface_render_depth(triangle.points[index].world, triangle.skirt);
    }
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
            // Strict comparison preserves equal-depth overwrite and alpha order.
            if fragment_depth < depth[y as usize * width as usize + x as usize] {
                first += plane.x[0];
                second += plane.x[1];
                continue;
            }
            let source = if let Some(sampler) = texture {
                let local = [
                    local_uv[0][0] * weights[0]
                        + local_uv[1][0] * weights[1]
                        + local_uv[2][0] * weights[2],
                    local_uv[0][1] * weights[0]
                        + local_uv[1][1] * weights[1]
                        + local_uv[2][1] * weights[2],
                ];
                let mut sample = sample_terrain_atlas(atlas, sampler, local);
                if let Some([second, third]) = blend {
                    let samples = [
                        sample,
                        sample_terrain_atlas(atlas, second, local),
                        sample_terrain_atlas(atlas, third, local),
                    ];
                    if landscape {
                        sample = crate::surface_mesh::landscape::texel(
                            samples,
                            crate::surface_mesh::landscape::interpolated_floor_weights(
                                floor_endpoints,
                                appearance,
                                weights,
                            ),
                            triangle.tint,
                            appearance,
                        );
                    } else {
                        for channel in 0..4 {
                            sample[channel] = (f64::from(samples[0][channel]) * first
                                + f64::from(samples[1][channel]) * weights[1]
                                + f64::from(samples[2][channel]) * weights[2])
                                .round() as u8;
                        }
                    }
                } else if landscape {
                    sample = crate::surface_mesh::landscape::texel(
                        [sample; 3],
                        [1.0, 0.0, 0.0],
                        triangle.tint,
                        appearance,
                    );
                }
                if landscape {
                    sample
                } else {
                    tint_sample(sample, triangle.tint)
                }
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

#[inline(never)]
fn sample_terrain_atlas(atlas: &[u8], address: crate::AtlasAddress, local: [f64; 2]) -> [u8; 4] {
    let rect = address.uv;
    let side = f64::from(GAME_ATLAS_SIDE);
    // Preserve the reference add/multiply order at texel boundaries.
    sample_atlas(
        atlas,
        address.page,
        (f64::from(rect[0]) * side + 0.5) + local[0] * (f64::from(rect[2]) * side - 1.0).max(0.0),
        (f64::from(rect[1]) * side + 0.5) + local[1] * (f64::from(rect[3]) * side - 1.0).max(0.0),
    )
}

struct RasterPlane {
    x: [f64; 2],
    y: [f64; 2],
    constant: [f64; 2],
}

impl RasterPlane {
    fn new(points: [SurfacePoint; 3]) -> Option<Self> {
        let [a, b, c] = [points[0].screen, points[1].screen, points[2].screen];
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
    if !valid_atlas(atlas) || layer_depth.is_nan() || layer_depth == f64::INFINITY {
        return;
    }
    let untinted = sprite.color == [1.0; 4];
    let center_x = (f64::from(sprite.position[0]) + 1.0) * viewport[0] * 0.5;
    let center_y = (1.0 - f64::from(sprite.position[1])) * viewport[1] * 0.5;
    let origin_x = center_x - f64::from(frame.size[0]) * 0.5;
    let origin_y = center_y - f64::from(frame.size[1]) * 0.5;
    let [u, v, source_width, source_height] = [
        f64::from(sprite.uv[0]),
        f64::from(sprite.uv[1]),
        f64::from(sprite.uv[2]),
        f64::from(sprite.uv[3]),
    ];
    for y in bounds[1]..bounds[3] {
        let texture_v =
            v + (f64::from(y) + 0.5 - origin_y) / f64::from(frame.size[1]) * source_height;
        for x in bounds[0]..bounds[2] {
            if layer_depth < depth[y as usize * width as usize + x as usize] {
                continue;
            }
            let horizontal = (f64::from(x) + 0.5 - origin_x) / f64::from(frame.size[0]);
            let texture_u = u + source_width * horizontal;
            let mut source = sample_atlas(
                atlas,
                sprite.pages[0],
                texture_u * f64::from(GAME_ATLAS_SIDE),
                texture_v * f64::from(GAME_ATLAS_SIDE),
            );
            if source[3] == 0 {
                continue;
            }
            if !untinted {
                source = transform_sample(source, sprite.color, [0.0; 4]);
            }
            write_fragment(x, y, width, layer_depth, source, color, depth);
        }
    }
}

pub(super) fn raster_selection(
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

fn valid_atlas(atlas: &[u8]) -> bool {
    atlas.len() == crate::GAME_ATLAS_BYTES
}

fn sample_atlas(atlas: &[u8], page: u32, x: f64, y: f64) -> [u8; 4] {
    // All callers validate once per primitive, not once per sampled texel.
    let side = GAME_ATLAS_SIDE as usize;
    // Saturating float-to-integer casts floor nonnegative coordinates and map
    // negative/NaN values to zero; integer min also handles positive infinity.
    let x = (x as usize).min(side - 1);
    let y = (y as usize).min(side - 1);
    let Some(start) = (page as usize)
        .checked_mul(crate::GAME_ATLAS_PAGE_BYTES)
        .and_then(|base| base.checked_add((y * side + x) * 4))
        .filter(|start| *start <= atlas.len().saturating_sub(4))
    else {
        return [0; 4];
    };
    [
        atlas[start],
        atlas[start + 1],
        atlas[start + 2],
        atlas[start + 3],
    ]
}

fn tint_sample(texel: [u8; 4], tint: u8) -> [u8; 4] {
    match tint {
        1..=3 => {
            let factor = [0.92, 0.78, 0.72][usize::from(tint - 1)];
            transform_sample(texel, [factor, factor, factor, 1.0], [0.0; 4])
        }
        4 => transform_sample(
            texel,
            [0.86, 0.86, 0.86, 1.0],
            [
                f32::from(WATER_TINT[0]) * 0.14,
                f32::from(WATER_TINT[1]) * 0.14,
                f32::from(WATER_TINT[2]) * 0.14,
                0.0,
            ],
        ),
        5..=10 | 12 | 21..=26 => crate::surface_mesh::procedural_tint(texel, tint),
        _ => texel,
    }
}

// Share the channel kernel across sprite multiplication and terrain tinting.
// Keep separate f32 multiply/add operations; do not fuse them or change rounding.
#[inline(never)]
fn transform_sample(mut source: [u8; 4], multiply: [f32; 4], offset: [f32; 4]) -> [u8; 4] {
    for channel in 0..4 {
        // Saturating casts clamp out-of-range values and map NaN to zero.
        source[channel] =
            (f32::from(source[channel]) * multiply[channel] + offset[channel]).round() as u8;
    }
    source
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
        let source_alpha = u32::from(source[3]);
        let target_alpha = 255 - source_alpha;
        // Denominator 255 has no half-integer ties. The closest result is
        // 1/510 from a rounding boundary, beyond the reference f32 error.
        for channel in 0..3 {
            color[offset + channel] = ((u32::from(source[channel]) * source_alpha
                + u32::from(color[offset + channel]) * target_alpha
                + 127)
                / 255) as u8;
        }
        color[offset + 3] = 255;
    }
    depth[pixel] = fragment_depth;
}

#[cfg(test)]
#[path = "tests/canvas_raster_reference.rs"]
mod reference;
#[cfg(test)]
#[path = "tests/canvas_raster.rs"]
mod tests;
