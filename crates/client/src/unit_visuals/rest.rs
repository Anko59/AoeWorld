//! Which pose a unit that was flagged as walking shows once it stands still.
use super::{Pose, military_poses, native_walk_speed};
use aoe_protocol::UnitKind;

/// The ticks over which a unit's travel is judged for a stall at `hz`: about
/// 0.6 s, but no more than the history retains once the presentation clock
/// lags the newest sample by up to two ticks.
pub(crate) fn stall_window_ticks(hz: f64) -> f64 {
    (hz * 0.6).clamp(2.0, 4.0)
}

/// The pose a unit flagged as walking shows when it has in fact stood still.
pub(crate) fn rest_pose(pose: Pose) -> Pose {
    use Pose::*;
    match pose {
        VillagerWalk => VillagerStand,
        ShepherdWalk => ShepherdStand,
        HunterWalk => HunterStand,
        MilitiaWalk => MilitiaStand,
        FishingShipWalk => FishingShipStand,
        HunterCarry => HunterCarryIdle,
        CarryWood => CarryWoodIdle,
        CarryFood => CarryFoodIdle,
        CarryGold => CarryGoldIdle,
        CarryStone => CarryStoneIdle,
        other => military_poses_of(other).unwrap_or(other),
    }
}

/// The standing pose of a military walking `pose`, if it is one.
fn military_poses_of(pose: Pose) -> Option<Pose> {
    UnitKind::ALL
        .into_iter()
        .filter_map(military_poses)
        .find(|poses| poses[1] == pose)
        .map(|poses| poses[0])
}

/// Whether a unit flagged as moving has in fact stood still: it covered under
/// a fifth of its native stride over `seconds`. Such a unit is drawn at rest
/// rather than frozen in mid-step.
pub(crate) fn is_stalled(kind: UnitKind, travelled: f64, seconds: f64) -> bool {
    travelled < native_walk_speed(kind) * seconds * 0.2
}
