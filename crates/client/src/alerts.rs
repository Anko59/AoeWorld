//! Which change of the viewer's own units and buildings is a new hit by somebody
//! else, and where. What was known before and what is told now decide, so a
//! state told again, or seen for the first time, is no alert.
use aoe_protocol::{GameplayBuildingState, GameplayUnitState, HitState};

/// A building is removed without the blow that destroyed it. The blow before is
/// at most this old: a lone trebuchet takes ten seconds between two shots.
const SIEGE_SECONDS: u64 = 12;

/// `own` finds the striker's side, which is work: only a new hit asks for it.
pub fn unit_hit(
    known: Option<&GameplayUnitState>,
    told: &GameplayUnitState,
    own: impl FnOnce(&HitState) -> bool,
) -> Option<[f64; 2]> {
    struck(known?.last_hit, told.last_hit, own).then(|| told.position.as_tiles())
}

pub fn building_hit(
    known: Option<&GameplayBuildingState>,
    told: &GameplayBuildingState,
    own: impl FnOnce(&HitState) -> bool,
) -> Option<[f64; 2]> {
    struck(known?.last_hit, told.last_hit, own).then(|| middle(told))
}

/// The wire names a lost building only by its id. One blow can take a wall or a
/// gate; anything else lost unhurt was cancelled, converted or spent. The caller
/// leaves out what the player deleted himself and what was replaced.
pub fn building_lost(
    known: &GameplayBuildingState,
    tick: u64,
    tick_hz: u32,
    own: impl FnOnce(&HitState) -> bool,
) -> Option<[f64; 2]> {
    let siege = SIEGE_SECONDS * u64::from(tick_hz);
    let told = match known.last_hit {
        Some(hit) if tick.saturating_sub(hit.tick.0) <= siege => !own(&hit),
        _ => known.kind.is_wall() || known.kind.is_gate(),
    };
    told.then(|| middle(known))
}

/// His own building told on that footprint in the same tick: stone over a
/// palisade, or a gate in a wall.
pub fn replaced(lost: &GameplayBuildingState, told: &[GameplayBuildingState]) -> bool {
    let old = lost.footprint;
    told.iter().any(|building| {
        let new = building.footprint;
        building.player == lost.player
            && old.min.x < new.max.x
            && new.min.x < old.max.x
            && old.min.y < new.max.y
            && new.min.y < old.max.y
    })
}

fn struck(
    known: Option<HitState>,
    told: Option<HitState>,
    own: impl FnOnce(&HitState) -> bool,
) -> bool {
    told.is_some_and(|hit| known != told && !own(&hit))
}

fn middle(building: &GameplayBuildingState) -> [f64; 2] {
    let (min, max) = (building.footprint.min, building.footprint.max);
    [
        (f64::from(min.x) + f64::from(max.x)) / 2.0,
        (f64::from(min.y) + f64::from(max.y)) / 2.0,
    ]
}
