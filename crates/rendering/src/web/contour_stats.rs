//! Depth normalization plus the cheap per-frame observations that explain a
//! missing unit contour: eligible instances, occluders and the depth span.
use super::Sprite;

/// What one frame's normalization saw. Counts exclude terrain (negative alpha).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct DepthStats {
    pub(crate) units: bool,
    pub(crate) occluders: bool,
    pub(crate) contours: u32,
    pub(crate) occluder_count: u32,
    /// World depth range of the whole frame before normalization.
    pub(crate) span: f32,
}

impl DepthStats {
    /// Hover, or units AND occluders; otherwise no coverage pass is needed.
    pub(crate) fn effects(self) -> bool {
        self.units && self.occluders
    }
}

/// Cumulative since the renderer was created, plus the last submitted frame.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct ContourStats {
    pub effect_frames: u32,
    pub plain_frames: u32,
    /// Instances carrying the unit-contour flag in the last frame.
    pub contour_instances: u32,
    pub occluder_instances: u32,
    /// Depth span of the last frame, in camera-relative world depth units.
    pub depth_span: f32,
    pub context_lost: u32,
    pub context_restored: u32,
}

impl ContourStats {
    pub(crate) fn record(&mut self, frame: DepthStats) {
        let frames = if frame.effects() {
            &mut self.effect_frames
        } else {
            &mut self.plain_frames
        };
        *frames = frames.saturating_add(1);
        self.contour_instances = frame.contours;
        self.occluder_instances = frame.occluder_count;
        self.depth_span = frame.span;
    }
}

pub(crate) fn normalize_depths_with_stats(instances: &mut [Sprite]) -> DepthStats {
    let mut stats = DepthStats::default();
    let mut range: Option<(f32, f32)> = None;
    for sprite in instances.iter() {
        if sprite.color[3] >= 0.0 {
            // H || (U && O) == (U || H) && (O || H): retain the existing
            // two flag checks, including for units, with no hover-only scan.
            stats.units |= sprite.pages[3] & (Sprite::UNIT_CONTOUR | 48) != 0;
            stats.occluders |= sprite.pages[3] & (Sprite::OCCLUDER | 48) != 0;
            stats.contours += u32::from(sprite.pages[3] & Sprite::UNIT_CONTOUR != 0);
            stats.occluder_count += u32::from(sprite.pages[3] & Sprite::OCCLUDER != 0);
        }
        for depth in sprite.depths[..3].iter().copied() {
            if depth.is_finite() {
                range = Some(match range {
                    Some((minimum, maximum)) => (minimum.min(depth), maximum.max(depth)),
                    None => (depth, depth),
                });
            }
        }
    }
    let (minimum, maximum) = range.unwrap_or((0.0, 0.0));
    let span = maximum - minimum;
    stats.span = span;
    for sprite in instances {
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
    stats
}
