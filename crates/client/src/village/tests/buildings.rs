use crate::playground::game::network::merge_buildings;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_protocol::{BuildingKind, GameplayBuildingState, MAX_ECONOMY_BUILDINGS};
use wasm_bindgen_test::wasm_bindgen_test;

fn building(id: usize) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(id as u32),
        player: PlayerId(0),
        kind: BuildingKind::TownCenter,
        footprint: TileRect {
            min: TileCoord::new(0, 0),
            max: TileCoord::new(4, 4),
        },
        training_queue: vec![],
    }
}

#[wasm_bindgen_test]
fn building_deltas_enforce_the_merged_limit_and_allow_replacements_and_removals() {
    let retained: Vec<_> = (0..MAX_ECONOMY_BUILDINGS).map(building).collect();
    assert!(merge_buildings(&retained, &[building(99)], &[]).is_none());
    assert_eq!(retained.len(), MAX_ECONOMY_BUILDINGS);
    let replaced = merge_buildings(&retained, &[building(0)], &[]).unwrap();
    assert_eq!(replaced.len(), MAX_ECONOMY_BUILDINGS);
    let swapped = merge_buildings(&retained, &[building(99)], &[EntityId(0)]).unwrap();
    assert_eq!(swapped.len(), MAX_ECONOMY_BUILDINGS);
    assert!(swapped.iter().any(|b| b.id == EntityId(99)));
    assert!(!swapped.iter().any(|b| b.id == EntityId(0)));
    assert!(merge_buildings(&swapped, &[building(100)], &[]).is_none());
}
