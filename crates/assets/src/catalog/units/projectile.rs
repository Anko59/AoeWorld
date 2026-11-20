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

/// DE sprite id of `p_bolt_x1.sld` (graphic 3391, unit 372 `Projectile GAL`):
/// 32 directions of one cell each, 11.25 degrees apart, clockwise on screen
/// from east (cell 0) through south (8), west (16) and north (24). Mirrored
/// rows reuse the west half.
pub const BOLT_SPRITE: u32 = 3812;
/// Native cell of each of the five rows: south, south-west, west, north-west, north.
pub(super) const BOLT_CELLS: [usize; 5] = [8, 12, 16, 20, 24];

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

/// The native flight pose of the projectile unit carried by the gameplay stream.
pub const fn projectile_pose(unit: u16) -> Option<Pose> {
    Some(match unit {
        368 | 374 => Pose::Cannonball,
        512 => Pose::GalleyArrow,
        477 | 478 => Pose::NativeArrow,
        367 | 372 => Pose::ScorpionBolt,
        373 | 627 => Pose::HeavyScorpionBolt,
        2636 => Pose::BoardingHook,
        656 => Pose::MangonelStone,
        369 => Pose::MangonelScatter,
        371 | 1932 => Pose::TrebuchetBoulder,
        380 => Pose::GunpowderShot,
        676 => Pose::FireStream,
        _ => return None,
    })
}

/// The signed arc multiplier of the DAT projectile record.
pub const fn projectile_arc(unit: u16) -> f64 {
    match unit {
        477 | 478 => -0.06,
        368 | 512 => -0.05,
        369 | 374 | 656 | 1932 => 0.4,
        371 => 0.65,
        372 | 373 | 380 => 0.05,
        676 => 0.45,
        2636 => -0.3,
        _ => 0.0,
    }
}

pub const fn projectile_effects(unit: u16) -> (Option<Pose>, Option<Pose>) {
    let muzzle = match unit {
        368 => Some(Pose::BombardCannonMuzzle),
        374 => Some(Pose::CannonGalleonMuzzle),
        380 => Some(Pose::HandCannoneerMuzzle),
        _ => None,
    };
    let impact = match unit {
        371 | 1932 => Some(Pose::TrebuchetImpact),
        367 | 368 | 372 | 373 | 374 | 380 | 627 | 656 | 676 => Some(Pose::ProjectileImpact),
        _ => None,
    };
    (muzzle, impact)
}

/// Frame of an attack animation at which the shot or the strike is released,
/// from the DE DAT unit's frame delay. `None` for poses that release nothing
/// mid-animation; this changes visual alignment, never simulation shot timing.
pub const fn release_frame(pose: Pose) -> Option<u8> {
    match pose {
        Pose::ArcherAttack | Pose::CrossbowmanAttack => Some(15),
        Pose::ArbalesterAttack => Some(20),
        Pose::SkirmisherAttack | Pose::EliteSkirmisherAttack => Some(19),
        Pose::HandCannoneerAttack => Some(15),
        Pose::TrebuchetUnpackedAttack => Some(24),
        Pose::MangonelAttack | Pose::OnagerAttack | Pose::SiegeOnagerAttack => Some(0),
        Pose::ScorpionAttack => Some(12),
        Pose::HeavyScorpionAttack => Some(6),
        Pose::BombardCannonAttack => Some(7),
        Pose::CavalryArcherAttack => Some(35),
        Pose::HeavyCavalryArcherAttack => Some(46),
        Pose::CamelRiderAttack | Pose::HeavyCamelRiderAttack => Some(20),
        Pose::BatteringRamAttack | Pose::CappedRamAttack | Pose::SiegeRamAttack => Some(0),
        Pose::CatapultGalleonAttack => Some(0),
        _ => None,
    }
}
