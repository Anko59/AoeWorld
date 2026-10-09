//! Terrain only: bounded pixel-quadrant box samples, retaining nearest coverage.
use super::*;

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

#[inline(never)]
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

/// Shared integer kernel; tap addressing remains specific to each sampler.
#[cfg(test)]
pub(super) fn add_tap(sums: &mut [u32; 4], tap: [u8; 4]) {
    for channel in 0..3 {
        sums[channel] += u32::from(tap[channel]) * u32::from(tap[3]);
    }
    sums[3] += u32::from(tap[3]);
}

#[cfg(test)]
pub(super) fn finish(center: [u8; 4], sums: [u32; 4]) -> [u8; 4] {
    let mut result = center;
    if sums[3] != 0 {
        for channel in 0..3 {
            result[channel] = ((sums[channel] + sums[3] / 2) / sums[3]) as u8;
        }
    }
    // Nearest center alpha alone determines coverage and depth.
    result
}

#[path = "filter/tests.rs"]
#[cfg(test)]
mod tests;
