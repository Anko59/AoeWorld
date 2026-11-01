//! Flight sprites chosen by direction, and the release frames of the
//! animations that fire them.
//!
//! The DAT arrow (SLP 3799) and javelin (SLP 3820) store pitch cells for each
//! of sixteen (arrow) or seventeen (javelin) directions in 11.25 degree steps
//! from south through west to north; east is the mirror. The atlas budget has
//! no room beyond one level-flight cell per five mirrored rows (south, south-
//! west, west, north-west, north), so the flight is drawn with the nearest.
use super::Pose;

/// Atlas cells per row.
pub const PROJECTILE_PHASES: u8 = 1;

/// Native cell of `row` for the projectile SLP `id` (3799 or 3820): the
/// level-flight cell measured on the real art.
pub(super) const fn source_cell(id: u32, row: usize) -> usize {
    if id == 3799 {
        [6, 49, 93, 137, 175][row]
    } else {
        [5, 45, 84, 125, 169][row]
    }
}

/// The packed facing of the flight cell nearest to a flight whose ground
/// direction is `(east, south)` in unsquished screen axes (tile `dx - dy`,
/// `dx + dy`).
pub fn projectile_facing(east: f64, south: f64) -> u8 {
    let azimuth = east.abs().atan2(south).to_degrees();
    let row = (azimuth / 45.0).round().clamp(0.0, 4.0) as u8;
    // Rows 1..=3 face south-west..north-west; the east is mirrored.
    match row {
        1..=3 if east > 0.0 => row,
        1..=3 => 8 - row,
        _ => row,
    }
}

/// Frame of an attack animation at which the shot or the strike is released,
/// from the DE DAT unit's frame delay. `None` for poses that release nothing
/// mid-animation; this changes visual alignment, never simulation shot timing.
pub const fn release_frame(pose: Pose) -> Option<u8> {
    match pose {
        Pose::ArcherAttack | Pose::CrossbowmanAttack => Some(15),
        Pose::SkirmisherAttack | Pose::EliteSkirmisherAttack => Some(19),
        _ => None,
    }
}
