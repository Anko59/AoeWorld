//! Terrain and object sprites: bounded pixel-quadrant box samples.
//! Terrain retains nearest coverage; sprites take the average coverage.
use super::*;
use crate::web::Sprite;

pub(super) fn gradients(plane: &RasterPlane, uv: [[f64; 2]; 3]) -> [[f64; 2]; 2] {
    [plane.x, plane.y].map(|weights| {
        std::array::from_fn(|axis| {
            (uv[0][axis] - uv[2][axis]) * weights[0] + (uv[1][axis] - uv[2][axis]) * weights[1]
        })
    })
}

pub(super) fn offsets(
    address: crate::AtlasAddress,
    gradient: [[f64; 2]; 2],
) -> Option<[[f64; 2]; 4]> {
    let side = f64::from(GAME_ATLAS_SIDE);
    let extent = [address.uv[2], address.uv[3]].map(|v| (f64::from(v) * side - 1.0).max(0.0));
    let footprint = gradient
        .iter()
        .map(|g| (g[0] * extent[0]).powi(2) + (g[1] * extent[1]).powi(2))
        .fold(0.0_f64, f64::max);
    if address.uv.iter().any(|v| !v.is_finite())
        || gradient.iter().flatten().any(|v| !v.is_finite())
        || !footprint.is_finite()
        || footprint <= 1.5625
    {
        return None;
    }
    Some(
        [[-0.25, -0.25], [-0.25, 0.25], [0.25, -0.25], [0.25, 0.25]].map(|[sx, sy]| {
            std::array::from_fn(|axis| sx * gradient[0][axis] + sy * gradient[1][axis])
        }),
    )
}

pub(super) fn sample(
    atlas: &[u8],
    address: crate::AtlasAddress,
    local: [f64; 2],
    offsets: Option<[[f64; 2]; 4]>,
) -> [u8; 4] {
    let center = sample_terrain_atlas(atlas, address, local);
    let Some(offsets) = offsets else {
        return center;
    };
    if local.iter().any(|v| !v.is_finite()) {
        return center;
    }
    let mut sums = [0_u32; 4];
    for offset in offsets {
        let uv = std::array::from_fn(|axis| (local[axis] + offset[axis]).clamp(0.0, 1.0));
        let tap = sample_terrain_atlas(atlas, address, uv);
        for channel in 0..3 {
            sums[channel] += u32::from(tap[channel]) * u32::from(tap[3]);
        }
        sums[3] += u32::from(tap[3]);
    }
    if sums[3] == 0 {
        return center;
    }
    let mut result = center;
    for channel in 0..3 {
        result[channel] = ((sums[channel] + sums[3] / 2) / sums[3]) as u8;
    }
    result
}

/// Object sprites minified below one and a quarter texel: the tap offsets of
/// the same bounded box kernel, from the uv step per screen pixel. The quad
/// is axis aligned, so the offsets are constant across the sprite.
pub(super) fn sprite_offsets(uv: [f32; 4], size: [f64; 2]) -> Option<[[f64; 2]; 4]> {
    let side = f64::from(GAME_ATLAS_SIDE);
    let gradient = [f64::from(uv[2]) / size[0], f64::from(uv[3]) / size[1]];
    let footprint = (gradient[0] * side)
        .powi(2)
        .max((gradient[1] * side).powi(2));
    if uv.map(f64::from).iter().any(|value| !value.is_finite())
        || gradient.iter().any(|value| !value.is_finite())
        || !footprint.is_finite()
        || footprint <= 1.5625
    {
        return None;
    }
    Some(
        [[-0.25, -0.25], [-0.25, 0.25], [0.25, -0.25], [0.25, 0.25]]
            .map(|[sx, sy]| [sx * gradient[0], sy * gradient[1]]),
    )
}

/// One minified sprite texel: the premultiplied box average of the four
/// taps, clamped into the sprite's own atlas rectangle (the uv span may be
/// negative for mirrored sprites, so order the corners). Transparent taps
/// contribute no colour; a fully transparent footprint keeps `center`.
pub(super) fn sprite_sample(
    atlas: &[u8],
    sprite: Sprite,
    uv: [f32; 4],
    texture: [f64; 2],
    offsets: &[[f64; 2]; 4],
    center: [u8; 4],
) -> [u8; 4] {
    let side = f64::from(GAME_ATLAS_SIDE);
    let rect = uv.map(f64::from);
    let mut lo = [0.0; 2];
    let mut hi = [0.0; 2];
    for axis in 0..2 {
        let start = rect[axis] * side;
        let end = (rect[axis] + rect[axis + 2]) * side;
        lo[axis] = start.min(end) + 0.5;
        hi[axis] = start.max(end) - 0.5;
    }
    let mut sums = [0_u32; 4];
    for offset in offsets {
        let point: [f64; 2] = std::array::from_fn(|axis| {
            aoe_core::clamp_f64(
                texture[axis] * side + offset[axis] * side,
                lo[axis],
                hi[axis],
            )
        });
        let tap = owner_sample(atlas, sprite, point[0], point[1]);
        for channel in 0..3 {
            sums[channel] += u32::from(tap[channel]) * u32::from(tap[3]);
        }
        sums[3] += u32::from(tap[3]);
    }
    if sums[3] == 0 {
        return center;
    }
    std::array::from_fn(|channel| {
        if channel < 3 {
            (f64::from(sums[channel]) / f64::from(sums[3]) + 0.5).floor() as u8
        } else {
            (f64::from(sums[3]) / 4.0 + 0.5).floor() as u8
        }
    })
}

#[path = "filter/tests.rs"]
#[cfg(test)]
mod tests;

/// Recolour only imported player-mask pixels, before the box kernel.
pub(super) fn owner_sample(atlas: &[u8], sprite: Sprite, x: f64, y: f64) -> [u8; 4] {
    let base = sample_atlas(atlas, sprite.pages[0], x, y);
    if sprite.pages[3] & Sprite::PLAYER_MASK == 0 {
        return base;
    }
    let mask = sample_atlas(atlas, sprite.pages[0] + 3, x, y);
    let colour = std::array::from_fn(|i| (sprite.terrain_blend[1][i] * 255.0).round() as u8);
    crate::player::player_texel(base, mask, colour)
}
