use crate::{
    GAME_ATLAS_SIDE,
    surface_mesh::{
        ProjectedSurfaceTriangle, directory, mix, relief, surface_render_depth,
        triangle_texture_coordinates,
    },
};
const WATER_TINT: [u8; 3] = [38, 113, 190];
#[path = "canvas_raster/filter.rs"]
mod filter;
#[path = "canvas_raster/junction.rs"]
mod junction;
#[path = "canvas_raster/mixing.rs"]
mod mixing;
#[path = "canvas_raster/plane.rs"]
mod plane;
#[path = "canvas_raster/selection.rs"]
mod selection;
use plane::RasterPlane;
pub(super) use selection::raster_selection;

#[cfg(test)]
pub(crate) fn raster_surface(
    triangle: &ProjectedSurfaceTriangle,
    atlas: &[u8],
    bounds: [u32; 4],
    width: u32,
    color: &mut [u8],
    depth: &mut [f64],
) {
    raster_surface_with_fog(triangle, atlas, bounds, width, color, depth, [1.0; 3]);
}

#[cfg(test)]
pub(super) fn raster_surface_with_fog(
    triangle: &ProjectedSurfaceTriangle,
    atlas: &[u8],
    bounds: [u32; 4],
    width: u32,
    color: &mut [u8],
    depth: &mut [f64],
    fog: [f32; 3],
) {
    raster_surface_region(triangle, atlas, bounds, bounds[0], width, color, depth, fog);
}

// Keep the original row's x origin when clipping damage. Incrementing from a
// different origin changes floating rounding at exact atlas/triangle edges.
pub(super) fn raster_surface_region(
    triangle: &ProjectedSurfaceTriangle,
    atlas: &[u8],
    bounds: [u32; 4],
    clip_left: u32,
    width: u32,
    color: &mut [u8],
    depth: &mut [f64],
    fog: [f32; 3],
) {
    let texture = triangle.texture_uv;
    if texture.is_some() && !valid_atlas(atlas) {
        return; // Every sampled alpha would be zero, including water/blend tint.
    }
    let blend = triangle.texture_blend;
    let full = mix::support::packed(triangle).map(|packed| (packed, mix::support::palette(packed)));
    let appearance = triangle.appearance;
    let landscape =
        appearance != 0 && triangle.tint <= 3 && triangle.vegetated() && !triangle.skirt;
    // Canonical endpoints are already quantized; normalize once per Canvas surface.
    let floor_endpoints = triangle
        .floor_strengths
        .map(|values| values.map(|value| f32::from(value) / 255.0));
    // A face over several world tiles resolves each pixel's own tile frame from
    // the atlas directory; without one there is nothing truthful to sample.
    let tiled = if texture.is_some() && triangle.mode() == directory::TILED {
        let layers = triangle.texture_materials.unwrap_or([triangle.material; 3]);
        let sheets = layers.map(|material| directory::sheet(atlas, material));
        let (Some(first), Some(second), Some(third)) = (sheets[0], sheets[1], sheets[2]) else {
            return;
        };
        Some((directory::locals(triangle), [first, second, third]))
    } else {
        None
    };
    let local_uv = match tiled {
        Some((locals, _)) => locals.map(directory::diamond),
        None => triangle_texture_coordinates(triangle.mode()),
    };
    let Some(plane) = RasterPlane::new(triangle.points) else {
        return;
    };
    let gradient = if landscape {
        filter::gradients(&plane, local_uv)
    } else {
        [[0.0; 2]; 2]
    };
    let primary_filter = if landscape {
        texture.and_then(|address| filter::offsets(address, gradient))
    } else {
        None
    };
    // Landscape blends have a zero third weight; metadata-free faces never
    // filter. Keep only the two offset payloads that can actually be consumed.
    let secondary_filter = if landscape {
        blend.and_then(|addresses| filter::offsets(addresses[0], gradient))
    } else {
        None
    };
    let shade = triangle.relief();
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
            if x < clip_left {
                first += plane.x[0];
                second += plane.x[1];
                continue;
            }
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
            let unlit = if let (Some(_), Some((packed, palette))) = (texture, full) {
                junction::sample(
                    triangle,
                    atlas,
                    packed,
                    palette,
                    weights,
                    gradient,
                    floor_endpoints,
                )
            } else if let Some((locals, sheets)) =
                tiled.filter(|_| landscape && triangle.mix != [0; 6])
            {
                mixing::sample(
                    triangle,
                    atlas,
                    locals,
                    sheets,
                    weights,
                    gradient,
                    floor_endpoints,
                )
            } else if let Some(sampler) = texture {
                let interpolate = |corners: [[f64; 2]; 3]| {
                    [0, 1].map(|axis| {
                        corners[0][axis] * weights[0]
                            + corners[1][axis] * weights[1]
                            + corners[2][axis] * weights[2]
                    })
                };
                let mut local = interpolate(local_uv);
                let (mut sampler, mut blend) = (sampler, blend);
                let (mut primary_filter, mut secondary_filter) = (primary_filter, secondary_filter);
                if let Some((locals, sheets)) = tiled {
                    let position = interpolate(locals);
                    let cell = position.map(f64::floor);
                    let tile = [0, 1]
                        .map(|axis| triangle.texture_tile[axis].wrapping_add(cell[axis] as i32));
                    local = directory::diamond([position[0] - cell[0], position[1] - cell[1]]);
                    sampler = sheets[0].frame(atlas, tile);
                    blend =
                        blend.map(|_| [sheets[1].frame(atlas, tile), sheets[2].frame(atlas, tile)]);
                    if landscape {
                        primary_filter = filter::offsets(sampler, gradient);
                        secondary_filter = blend.and_then(|b| filter::offsets(b[0], gradient));
                    }
                }
                let mut sample = filter::sample(atlas, sampler, local, primary_filter);
                if let Some([second, third]) = blend {
                    let samples = [
                        sample,
                        filter::sample(atlas, second, local, secondary_filter),
                        if landscape {
                            sample
                        } else {
                            sample_terrain_atlas(atlas, third, local)
                        },
                    ];
                    let floor = crate::surface_mesh::landscape::interpolated_floor_weights(
                        floor_endpoints,
                        appearance,
                        weights,
                    );
                    if landscape {
                        sample = crate::surface_mesh::landscape::texel(
                            samples,
                            floor,
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
                wear_sample(
                    if landscape {
                        sample
                    } else {
                        tint_sample(sample, triangle.tint)
                    },
                    triangle.wear(),
                )
            } else {
                [
                    (triangle.color[0] * 255.0).round() as u8,
                    (triangle.color[1] * 255.0).round() as u8,
                    (triangle.color[2] * 255.0).round() as u8,
                    u8::MAX,
                ]
            };
            let vertex_shade = weights[0] * f64::from(shade[0])
                + weights[1] * f64::from(shade[1])
                + weights[2] * f64::from(shade[2]);
            let mut source = relief::shade_sample(unlit, vertex_shade as f32);
            let factor = weights
                .iter()
                .zip(fog)
                .map(|(w, f)| *w * f64::from(f))
                .sum::<f64>();
            for c in &mut source[..3] {
                *c = (f64::from(*c) * factor).round() as u8;
            }
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
    let sample = |local: [f64; 2]| {
        sample_atlas(
            atlas,
            address.page,
            (f64::from(rect[0]) * side + 0.5)
                + local[0] * (f64::from(rect[2]) * side - 1.0).max(0.0),
            (f64::from(rect[1]) * side + 0.5)
                + local[1] * (f64::from(rect[3]) * side - 1.0).max(0.0),
        )
    };
    let center = sample(local);
    let distance = (local[0] - 0.5).abs() + (local[1] - 0.5).abs();
    let extent = f64::from(rect[2].min(rect[3])) * side;
    let diamond = ((f64::from(rect[2]) - 2.0 * f64::from(rect[3])) * side + 1.0).abs() <= 1.0;
    // The mesh owns coverage. Only repair the native diamond's one-texel
    // staircase; retain transparency elsewhere and leave object alpha alone.
    if center[3] == 0
        && diamond
        && rect.iter().all(|v| v.is_finite())
        && extent > 2.0
        && (0.47..=0.50001).contains(&distance)
    {
        sample(local.map(|v| v + (0.5 - v) * (2.0 / extent)))
    } else {
        center
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
    // Terrain also arrives as quads: retain its original nearest lookup and
    // floor/ceil coverage. Only object quads use the minification kernel.
    let terrain = sprite.pages[3] & crate::web::Sprite::NEAREST_TERRAIN_QUAD != 0;
    let offsets = if terrain {
        None
    } else {
        filter::sprite_offsets(
            sprite.uv,
            [f64::from(frame.size[0]), f64::from(frame.size[1])],
        )
    };
    for y in bounds[1]..bounds[3] {
        let texture_v =
            v + (f64::from(y) + 0.5 - origin_y) / f64::from(frame.size[1]) * source_height;
        for x in bounds[0]..bounds[2] {
            if layer_depth < depth[y as usize * width as usize + x as usize] {
                continue;
            }
            // Object quad coverage matches GPU pixel centres. Terrain keeps
            // its original floor/ceil bounds, including edge samples.
            let horizontal = (f64::from(x) + 0.5 - origin_x) / f64::from(frame.size[0]);
            let vertical = (f64::from(y) + 0.5 - origin_y) / f64::from(frame.size[1]);
            if !terrain && (!(0.0..1.0).contains(&horizontal) || !(0.0..1.0).contains(&vertical)) {
                continue;
            }
            let texture_u = u + source_width * horizontal;
            if sprite.pages[3] & 128 != 0 && (horizontal - 0.5).abs() + (vertical - 0.5).abs() > 0.5
            {
                continue;
            }
            let mut source = filter::owner_sample(
                atlas,
                sprite,
                texture_u * f64::from(GAME_ATLAS_SIDE),
                texture_v * f64::from(GAME_ATLAS_SIDE),
            );
            if let Some(offsets) = &offsets {
                source = filter::sprite_sample(
                    atlas,
                    sprite,
                    sprite.uv,
                    [texture_u, texture_v],
                    offsets,
                    source,
                );
            }
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

/// Footprint wear darkens the sampled ground exactly as the GPU shaders do.
fn wear_sample(texel: [u8; 4], wear: u8) -> [u8; 4] {
    if wear == 0 {
        return texel;
    }
    let strength = f32::from(wear) / f32::from(crate::surface_mesh::WEAR_LEVELS);
    let [red, green, blue] = crate::surface_mesh::WEAR_DARKEN;
    transform_sample(
        texel,
        [
            1.0 - red * strength,
            1.0 - green * strength,
            1.0 - blue * strength,
            1.0,
        ],
        [0.0; 4],
    )
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
