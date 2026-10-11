//! Soft borders between every pair of ground materials: the contract only.
//! Mesh assignment, layer selection and the three renderers are not written
//! yet, so the mixing tests are red until the implementation lands.
use super::ProjectedSurfaceTriangle;

#[cfg(test)]
pub(crate) mod fixture;

impl ProjectedSurfaceTriangle {
    /// Faces drawn through the appearance word: palette, bed and filtered sheets.
    pub(crate) fn vegetated(&self) -> bool {
        matches!(self.material, 0 | 1 | 2 | 6) || self.mix != [0; 6]
    }
}

/// Overlay order, low to high: a higher material spills over a lower one.
/// Bare hard ground is lowest, vegetation creeps over it and snow covers all.
#[cfg(test)]
pub(crate) fn rank(material: u8) -> u8 {
    match material {
        8 => 0,
        4 => 1,
        9 => 2,
        3 => 3,
        2 => 4,
        1 => 5,
        0 => 6,
        7 => 7,
        _ => 8,
    }
}

/// World-anchored value noise in 0..=1.
#[cfg(test)]
pub(crate) fn noise(_tile: [i32; 2], _inside: [f32; 2]) -> f32 {
    0.5
}

/// Layer weights from the interpolated overlay share, middle share and bed.
#[cfg(test)]
pub(crate) fn weights(_shares: [f32; 3], _middle: bool, _noise: f32) -> [f32; 3] {
    [1.0, 0.0, 0.0]
}

/// One layer's colour before blending.
#[cfg(test)]
pub(crate) fn layer(texel: [u8; 4], _material: u8) -> [f32; 4] {
    texel.map(f32::from)
}
