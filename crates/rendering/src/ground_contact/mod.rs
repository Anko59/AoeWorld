//! Flat-ground clearance follows the displayed frame below its native hotspot.
/// One world-depth unit spans half a tile vertically. Preserve the ordinary
/// offset until the frame reaches farther below its hotspot; one pixel of
/// slack keeps its last raster row ahead of coplanar ground on every backend.
pub(crate) fn frame_clearance(minimum: f64, height: f32, anchor: f32) -> f64 {
    minimum.max((f64::from(height - anchor) + 1.0) / (aoe_core::ISO_TILE_HEIGHT / 2.0))
}

#[cfg(test)]
mod tests;
