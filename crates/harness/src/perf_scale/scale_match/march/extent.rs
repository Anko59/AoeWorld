//! Arrival radius of a formation order, from the slots the simulation lays out.
use aoe_sdk::UnitKind;
use serde_json::Value;

/// Tiles past the farthest slot within which a member counts as arrived.
const MARGIN_TILES: f64 = 3.0;
/// The simulation's unscaled layout spacing (`formation/layout.rs`).
const LAYOUT_GAP: f64 = 1040.0;
/// Fixed subunits per tile, and the cap of the per-tick step in a slot gap.
const SUBUNITS: f64 = 1024.0;
const MAX_STEP: f64 = 512.0;

/// The simulation's kind of the wire protocol's (same variant name).
fn simulation_kind(kind: UnitKind) -> Option<aoe_simulation::UnitKind> {
    let name = format!("{kind:?}");
    aoe_simulation::UnitKind::ALL
        .into_iter()
        .find(|k| format!("{k:?}") == name)
}

/// Farthest slot of the real group, in tiles. The server's probe reports the
/// unscaled layout; the simulation spaces slots by
/// `2 * diameter + step + 16` (`formation/orders.rs`), so a group with wide
/// bodies (rams, mangonels) or fast units spreads further than that.
pub(super) fn slot_extent_tiles(probe: &Value, kinds: &[UnitKind]) -> f64 {
    let unscaled = probe["extent_tiles"].as_f64().unwrap_or(0.0);
    let diameter = kinds
        .iter()
        .filter_map(|k| simulation_kind(*k))
        .map(|k| f64::from(k.collision_radius() * 2))
        .fold(0.0, f64::max);
    let step = kinds
        .iter()
        .filter_map(|k| probe["speeds_tiles_per_tick"][format!("{k:?}")].as_f64())
        .map(|tiles| (tiles * SUBUNITS - 1e-6).ceil().min(MAX_STEP))
        .fold(0.0, f64::max);
    let gap = diameter * 2.0 + step + 16.0;
    unscaled * gap / LAYOUT_GAP
}

/// Arrival tolerance: farthest actual slot plus three tiles.
pub(super) fn arrival_tolerance(probe: &Value, kinds: &[UnitKind]) -> f64 {
    slot_extent_tiles(probe, kinds) + MARGIN_TILES
}
