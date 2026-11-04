//! Outline of each Steward's territory, mirroring the server's rule: a point
//! belongs to the nearest active town_center of the player within the radius, ties
//! to the lower ID. Where two cells meet, each cell's outline is pulled back
//! from the shared bisector so the boundary reads as a double line.
use aoe_core::EntityId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TownCenter {
    pub id: EntityId,
    pub center: [f64; 2],
    pub active: bool,
}

/// Half the gap between the two lines at a shared border, in tiles.
const BORDER_HALF_GAP: f64 = 0.4;

/// Closed outline (tile coordinates) of `me`'s cell, sampled by `steps` rays.
/// An inactive town_center draws its whole circle: it claims nothing.
pub(super) fn outline(
    me: &TownCenter,
    all: &[TownCenter],
    radius: f64,
    steps: usize,
) -> Vec<[f64; 2]> {
    (0..steps)
        .map(|k| {
            let angle = std::f64::consts::TAU * k as f64 / steps as f64;
            let (sin, cos) = angle.sin_cos();
            let mut reach = radius;
            if me.active {
                for rival in all.iter().filter(|r| r.active && r.id != me.id) {
                    let d = [
                        rival.center[0] - me.center[0],
                        rival.center[1] - me.center[1],
                    ];
                    let len = d[0].hypot(d[1]);
                    let along = d[0] * cos + d[1] * sin;
                    if len > 0.0 && len < 2.0 * radius && along > 1e-9 {
                        let half = (len / 2.0 - BORDER_HALF_GAP).max(0.0);
                        reach = reach.min(half * len / along);
                    }
                }
            }
            [me.center[0] + reach * cos, me.center[1] + reach * sin]
        })
        .collect()
}

#[cfg(test)]
#[path = "territory_tests.rs"]
mod tests;
