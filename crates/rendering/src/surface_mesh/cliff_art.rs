//! Opaque, visually reviewed rock interiors from the original cliff source 226.
//! One formation spans three tiles, rather than repeating its middle per tile.
use super::*;

pub(super) fn apply(face: &mut ProjectedSurfaceTriangle, art: &GameArt) -> bool {
    if !face.skirt || face.tint != 13 || art.scenery.cliff_faces.len() != 25 {
        return false;
    }
    let east = face.order < 4;
    let parallel = usize::from(east);
    let cell = face.tile.map(|n| n.div_euclid(3) as u32);
    let key = hash(cell[0] ^ cell[1].rotate_left(16));
    let group = if east {
        [4, 5, 16, 17, 24]
    } else {
        [1, 2, 18, 15, 19]
    };
    let slot = group[key as usize % group.len()];
    let frame = art.scenery.cliff_faces[slot];
    if frame.size.iter().any(|v| !v.is_finite() || *v < 8.0) {
        return false;
    }
    // Pixel rectangles have two texels of margin inside the maximal fully
    // opaque interiors: preserve fractures/highlights, exclude silhouette holes.
    let [left, top, width, height] = match slot {
        1 => [62.0, 39.0, 139.0, 70.0],
        2 => [64.0, 41.0, 135.0, 71.0],
        4 => [61.0, 45.0, 164.0, 54.0],
        5 => [57.0, 42.0, 166.0, 57.0],
        15 => [89.0, 35.0, 134.0, 85.0],
        16 => [100.0, 42.0, 123.0, 55.0],
        17 => [56.0, 45.0, 168.0, 55.0],
        18 => [105.0, 43.0, 136.0, 56.0],
        19 => [57.0, 41.0, 152.0, 58.0],
        _ => [54.0, 48.0, 172.0, 44.0],
    };
    let extent = face.points.iter().map(|p| p.world[parallel]);
    let lo = extent.clone().fold(f64::INFINITY, f64::min);
    let hi = extent.fold(f64::NEG_INFINITY, f64::max);
    let span = (hi - lo).round().clamp(1.0, 3.0) as f32;
    let phase = (face.tile[parallel].rem_euclid(3) as f32).min(3.0 - span);
    let origin = [
        (left + width * phase / 3.0) / frame.size[0],
        top / frame.size[1],
    ];
    let extent = [width * span / 3.0 / frame.size[0], height / frame.size[1]];
    let mut address = frame.atlas;
    // Existing rectangular modes sample 0.3..0.7. Express the selected source
    // region through that mapping without changing the shared packet or shader.
    for axis in 0..2 {
        address.uv[axis] += frame.atlas.uv[axis + 2] * (origin[axis] - 0.75 * extent[axis]);
        address.uv[axis + 2] *= extent[axis] / 0.4;
    }
    face.texture_uv = Some(address);
    face.texture_blend = None;
    face.texture_materials = None;
    face.appearance = 0;
    face.tint = 0;
    true
}

pub(super) fn hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}
