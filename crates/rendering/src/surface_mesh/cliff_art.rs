//! Opaque rock interiors from the native cliff art, spread along the wall.
use super::*;

const DE_SPAN: i32 = 4;

pub(super) fn available(art: &GameArt) -> bool {
    art.scenery.cliff_faces.len() == 25
        || art
            .environment
            .iter()
            .any(|c| c.unit_id == 264 && !c.parts.is_empty())
}

fn native_variant(face: &ProjectedSurfaceTriangle) -> (usize, [f32; 4]) {
    // Opaque pixel bounds in n_cliff_default_x1.sld, inset from the
    // silhouette. Use separate banks for the two authored lighting directions.
    let parallel = usize::from(face.order < 4);
    let variants = if parallel == 1 {
        [
            (4, [46.0, 51.0, 155.0, 142.0]),
            (5, [15.0, 73.0, 130.0, 159.0]),
            (16, [135.0, 17.0, 214.0, 115.0]),
            (17, [21.0, 45.0, 142.0, 114.0]),
        ]
    } else {
        [
            (1, [75.0, 73.0, 190.0, 159.0]),
            (2, [38.0, 51.0, 147.0, 142.0]),
            (15, [20.0, 48.0, 193.0, 133.0]),
            (18, [58.0, 32.0, 178.0, 101.0]),
        ]
    };
    let along = face.tile[parallel].div_euclid(DE_SPAN) as u32;
    let across = face.tile[1 - parallel] as u32;
    let key = hash(along.wrapping_mul(0x9e37_79b9) ^ across.rotate_left(16));
    variants[key as usize % variants.len()]
}

fn native_crop(face: &ProjectedSurfaceTriangle, art: &GameArt) -> Option<AtlasAddress> {
    let (slot, [left, top, right, bottom]) = native_variant(face);
    let frame = art.resident_frame(*art.scenery.cliff_faces.get(slot)?)?;
    if !(right <= frame.size[0] && bottom <= frame.size[1]) {
        return None;
    }
    let parallel = usize::from(face.order < 4);
    let phase = face.tile[parallel].rem_euclid(DE_SPAN) as f32;
    let remaining = DE_SPAN as f32 - phase;
    // The phase leaves at least one tile. Use static clamp bounds so WASM
    // does not link float formatting for an invalid-bounds panic.
    let span = wall_span(face, parallel).clamp(1.0, DE_SPAN as f32);
    let span = if span > remaining { remaining } else { span };
    // South edges run from east to west within each quad.
    let start = if parallel == 1 {
        phase
    } else {
        DE_SPAN as f32 - phase - span
    };
    let width = (right - left) / DE_SPAN as f32;
    let origin = [(left + width * start) / frame.size[0], top / frame.size[1]];
    let extent = [width * span / frame.size[0], (bottom - top) / frame.size[1]];
    Some(texture_rect(frame.atlas, origin, extent))
}

pub(super) fn apply(face: &mut ProjectedSurfaceTriangle, art: &GameArt) -> bool {
    if !face.skirt || face.tint != 13 || !available(art) {
        return false;
    }
    if art.environment.iter().any(|c| c.unit_id == 264) {
        let Some(address) = native_crop(face, art) else {
            return false;
        };
        return paint(face, address);
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
    let span = wall_span(face, parallel).clamp(1.0, 3.0);
    let phase = (face.tile[parallel].rem_euclid(3) as f32).min(3.0 - span);
    let origin = [
        (left + width * phase / 3.0) / frame.size[0],
        top / frame.size[1],
    ];
    let extent = [width * span / 3.0 / frame.size[0], height / frame.size[1]];
    paint(face, texture_rect(frame.atlas, origin, extent))
}

fn wall_span(face: &ProjectedSurfaceTriangle, axis: usize) -> f32 {
    let extent = face.points.iter().map(|p| p.world[axis]);
    let lo = extent.clone().fold(f64::INFINITY, f64::min);
    let hi = extent.fold(f64::NEG_INFINITY, f64::max);
    (hi - lo).round() as f32
}

fn texture_rect(mut address: AtlasAddress, origin: [f32; 2], extent: [f32; 2]) -> AtlasAddress {
    // Existing rectangular modes sample 0.3..0.7. Express the selected source
    // region through that mapping without changing the shared packet or shader.
    for axis in 0..2 {
        address.uv[axis] += address.uv[axis + 2] * (origin[axis] - 0.75 * extent[axis]);
        address.uv[axis + 2] *= extent[axis] / 0.4;
    }
    address
}

fn paint(face: &mut ProjectedSurfaceTriangle, address: AtlasAddress) -> bool {
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

/// The prepared mesh is already view-culled; request only its native cliff cells.
pub fn scene_cliff_native_frames(art: &GameArt, faces: &[ProjectedSurfaceTriangle]) -> Vec<u32> {
    if !art.environment.iter().any(|c| c.unit_id == 264) {
        return Vec::new();
    }
    #[cfg(feature = "frame-bench")]
    crate::frame_cost::add(crate::frame_cost::Counter::CliffFaces, faces.len());
    let frames = &art.scenery.cliff_faces;
    let mut out = std::collections::BTreeSet::new();
    for face in faces.iter().filter(|f| f.skirt && f.tint == 13) {
        if let Some(frame) = frames.get(native_variant(face).0)
            && frame.stream != 0
        {
            out.insert(frame.stream);
        }
    }
    out.into_iter().collect()
}
