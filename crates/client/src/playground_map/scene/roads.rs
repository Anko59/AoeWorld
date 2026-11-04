//! Dirt tracks use the pack's native terrain sheet and shared terrain blending.
use aoe_protocol::{BuildingKind, ConstructionState, GameplayBuildingState};
use aoe_rendering::SceneTerrain;

pub(super) fn paint(terrain: &mut [SceneTerrain], buildings: &[GameplayBuildingState]) {
    let roads: std::collections::BTreeSet<_> = buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Road && b.construction == ConstructionState::Complete)
        .map(|b| (b.footprint.min.x, b.footprint.min.y))
        .collect();
    if roads.is_empty() {
        return;
    }
    for tile in terrain {
        if tile.surface.water == 0
            && roads.contains(&(
                tile.position[0].floor() as i32,
                tile.position[1].floor() as i32,
            ))
        {
            tile.material = 2;
            if let Some(appearance) = &mut tile.appearance {
                appearance.canopy_strength = 0;
                appearance.floor_strength = 0;
            }
        }
    }
}
