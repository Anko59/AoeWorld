//! Filter the directional slope before quantization, never terrain classes.
use super::{FLAT_SHADE, SHADE_STEP_PER_MILLE};

/// A radius of four source facets removes cell borders without washing out
/// Finland's low relief. At least four raster pixels suppress subpixel aliasing
/// on continental maps; the axis/8 cap preserves broad hills on coarse grids.
/// The no-source fallback has eight-tile height cells. Tiny rasters use a
/// smaller mirrored window; a one-pixel raster has no slope.
pub(super) fn radius(samples: u16, tiles: u64, elevation_axis: u16) -> usize {
    let side = usize::from(samples);
    if side <= 1 {
        return 0;
    }
    let facet_radius = if elevation_axis == 0 {
        (32 * u64::from(samples)).div_ceil(tiles)
    } else {
        (4 * u64::from(samples)).div_ceil(u64::from(elevation_axis))
    };
    (facet_radius as usize)
        .clamp(4, (side / 8).max(4))
        .min(side - 1)
}

/// North-west light: ground rising toward the south-east is lit. Average the
/// unquantized rise with a fixed, mirror-padded separable box. Sliding sums
/// cost O(side²), independent of source/world size and filter radius. Two i64
/// buffers retain precision through both passes; divide only at the end.
pub(super) fn shades(heights: &[i32], side: usize, spacing: i64, radius: usize) -> Vec<u8> {
    let mut rises = Vec::with_capacity(heights.len());
    for y in 0..side {
        for x in 0..side {
            let axis = |before: usize, after: usize, low: usize, high: usize| {
                if high == low {
                    0
                } else {
                    (i64::from(heights[after]) - i64::from(heights[before])) / (high - low) as i64
                }
            };
            let (left, right) = (x.saturating_sub(1), (x + 1).min(side - 1));
            let (up, down) = (y.saturating_sub(1), (y + 1).min(side - 1));
            rises.push(
                axis(y * side + left, y * side + right, left, right)
                    + axis(up * side + x, down * side + x, up, down),
            );
        }
    }
    let mut horizontal = vec![0; heights.len()];
    box_sum(&rises, &mut horizontal, side, radius, false);
    box_sum(&horizontal, &mut rises, side, radius, true);
    let area = ((2 * radius + 1) as i64).pow(2);
    // i32 heights, side <= 512, radius <= 64: even the two summed windows
    // fit in i64. Only the physical-scale conversion needs saturation.
    rises
        .into_iter()
        .map(|sum| {
            let per_mille = (sum / area).saturating_mul(10_000) / spacing;
            (i64::from(FLAT_SHADE) + per_mille / SHADE_STEP_PER_MILLE).clamp(0, 15) as u8
        })
        .collect()
}

fn reflected(position: isize, side: usize) -> usize {
    if side == 1 {
        return 0;
    }
    let period = 2 * (side - 1);
    let position = position.rem_euclid(period as isize) as usize;
    position.min(period - position)
}

fn box_sum(input: &[i64], output: &mut [i64], side: usize, radius: usize, vertical: bool) {
    let radius = radius as isize;
    for line in 0..side {
        let index = |position| {
            let position = reflected(position, side);
            if vertical {
                position * side + line
            } else {
                line * side + position
            }
        };
        let mut sum: i64 = (-radius..=radius).map(|p| input[index(p)]).sum();
        output[index(0)] = sum;
        for position in 1..side {
            let position = position as isize;
            sum += input[index(position + radius)] - input[index(position - radius - 1)];
            output[index(position)] = sum;
        }
    }
}
