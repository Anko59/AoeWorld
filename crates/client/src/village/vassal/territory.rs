//! Outline of each Vassal's territory, mirroring the server's rule: a point
//! belongs to the nearest active castle of the player within the radius, ties
//! to the lower ID. Where two cells meet, each cell's outline is pulled back
//! from the shared bisector so the boundary reads as a double line.
use aoe_core::EntityId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Castle {
    pub id: EntityId,
    pub center: [f64; 2],
    pub active: bool,
}

/// Half the gap between the two lines at a shared border, in tiles.
const BORDER_HALF_GAP: f64 = 0.4;

/// Closed outline (tile coordinates) of `me`'s cell, sampled by `steps` rays.
/// An inactive castle draws its whole circle: it claims nothing.
pub(super) fn outline(me: &Castle, all: &[Castle], radius: f64, steps: usize) -> Vec<[f64; 2]> {
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
mod tests {
    use super::*;

    fn castle(id: u32, x: f64, active: bool) -> Castle {
        Castle {
            id: EntityId(id),
            center: [x, 90.0],
            active,
        }
    }
    fn nearest(point: [f64; 2], all: &[Castle]) -> Option<EntityId> {
        all.iter()
            .filter(|c| c.active)
            .min_by(|a, b| {
                let d = |c: &Castle| (c.center[0] - point[0]).hypot(c.center[1] - point[1]);
                d(a).total_cmp(&d(b)).then(a.id.cmp(&b.id))
            })
            .map(|c| c.id)
    }

    #[test]
    fn a_lone_castle_draws_its_full_circle() {
        let a = castle(1, 90.0, true);
        for p in outline(&a, &[a], 100.0, 64) {
            assert!(((p[0] - 90.0).hypot(p[1] - 90.0) - 100.0).abs() < 1e-9);
        }
    }

    #[test]
    fn overlapping_castles_meet_at_the_bisector_with_a_double_line() {
        let (a, b) = (castle(1, 90.0, true), castle(2, 150.0, true));
        let all = [a, b];
        let (left, right) = (outline(&a, &all, 100.0, 128), outline(&b, &all, 100.0, 128));
        // The bisector is x = 120; each outline stops short of it by the gap.
        let a_max = left.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        let b_min = right.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
        assert!((a_max - (120.0 - BORDER_HALF_GAP)).abs() < 1e-6, "{a_max}");
        assert!((b_min - (120.0 + BORDER_HALF_GAP)).abs() < 1e-6, "{b_min}");
        assert!(b_min - a_max > 0.5, "two distinct lines");
        // Every vertex lies in its own cell, so the cells never overlap.
        assert!(left.iter().all(|p| nearest(*p, &all) == Some(a.id)));
        assert!(right.iter().all(|p| nearest(*p, &all) == Some(b.id)));
    }

    #[test]
    fn castles_beyond_two_radii_do_not_interact_and_inactive_ones_claim_nothing() {
        let (a, far) = (castle(1, 90.0, true), castle(2, 300.0, true));
        assert_eq!(
            outline(&a, &[a, far], 100.0, 32),
            outline(&a, &[a], 100.0, 32)
        );
        let idle = castle(3, 150.0, false);
        assert_eq!(
            outline(&a, &[a, idle], 100.0, 32),
            outline(&a, &[a], 100.0, 32)
        );
        let full = outline(&idle, &[a, idle], 100.0, 32);
        assert!(
            full.iter()
                .all(|p| ((p[0] - 150.0).hypot(p[1] - 90.0) - 100.0).abs() < 1e-9)
        );
    }
}
