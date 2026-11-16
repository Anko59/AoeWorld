//! Camera-relative world depths to depth-buffer values.
use super::Sprite;

/// The finite depth range of a packet list, with its occlusion flags.
///
/// A frame is normalized with the range of all its packets. Gathering it is
/// separate from applying it, so a scene's ground can keep the range of its
/// own packets and take part in every later frame's range without another
/// visit: a minimum and a maximum do not depend on the order of their terms.
#[derive(Clone, Copy, Default)]
pub(crate) struct DepthRange {
    pub(crate) range: Option<(f32, f32)>,
    units: bool,
    occluders: bool,
}

impl DepthRange {
    pub(crate) fn include(&mut self, sprite: &Sprite) {
        if sprite.color[3] >= 0.0 {
            // H || (U && O) == (U || H) && (O || H): retain the existing
            // two flag checks, including for units, with no hover-only scan.
            self.units |= sprite.pages[3] & (Sprite::UNIT_CONTOUR | 48) != 0;
            self.occluders |= sprite.pages[3] & (Sprite::OCCLUDER | 48) != 0;
        }
        for depth in sprite.depths[..3].iter().copied() {
            if depth.is_finite() {
                self.range = Some(match self.range {
                    Some((minimum, maximum)) => (minimum.min(depth), maximum.max(depth)),
                    None => (depth, depth),
                });
            }
        }
    }

    /// Whether the frame holds both a unit to outline and something hiding it.
    pub(crate) fn effects(self) -> bool {
        self.units && self.occluders
    }

    pub(crate) fn normalize(self, sprite: &mut Sprite) {
        let (minimum, maximum) = self.range.unwrap_or((0.0, 0.0));
        let span = maximum - minimum;
        // Variant-tagged surface w carries an exact numeric 24-bit floor packet,
        // not geometry depth. Manual, metadata-free and object packets keep depth normalization.
        let count = if sprite.color[3] < 0.0
            && sprite.pages[3] & (crate::surface_mesh::landscape::INTERPOLATED_FLOOR | 1)
                == (crate::surface_mesh::landscape::INTERPOLATED_FLOOR | 1)
        {
            3
        } else {
            4
        };
        for depth in &mut sprite.depths[..count] {
            *depth = if *depth == f32::NEG_INFINITY {
                1.0
            } else if !depth.is_finite() {
                0.0
            } else if span <= f32::EPSILON {
                0.5
            } else {
                ((maximum - *depth) / span).clamp(0.0, 1.0)
            };
        }
    }
}

pub(crate) fn normalize_depths(instances: &mut [Sprite]) -> bool {
    let mut range = DepthRange::default();
    for sprite in instances.iter() {
        range.include(sprite);
    }
    for sprite in instances {
        range.normalize(sprite);
    }
    range.effects()
}
