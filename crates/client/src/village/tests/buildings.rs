use crate::playground::game::network::merge_buildings;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_protocol::{BuildingKind, ConstructionState, GameplayBuildingState, MAX_ECONOMY_BUILDINGS};
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
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 2400,
        max_hp: 2400,
        los: 8,
        food_remaining: None,
        research: None,
        training_queue: vec![],
        last_hit: None,
    }
}

#[wasm_bindgen_test]
fn building_deltas_enforce_the_merged_limit_and_allow_replacements_and_removals() {
    let retained: Vec<_> = (0..MAX_ECONOMY_BUILDINGS).map(building).collect();
    assert!(merge_buildings(&retained, &[building(MAX_ECONOMY_BUILDINGS + 99)], &[]).is_none());
    assert_eq!(retained.len(), MAX_ECONOMY_BUILDINGS);
    let replaced = merge_buildings(&retained, &[building(0)], &[]).unwrap();
    assert_eq!(replaced.len(), MAX_ECONOMY_BUILDINGS);
    let swapped = merge_buildings(
        &retained,
        &[building(MAX_ECONOMY_BUILDINGS + 99)],
        &[EntityId(0)],
    )
    .unwrap();
    assert_eq!(swapped.len(), MAX_ECONOMY_BUILDINGS);
    assert!(
        swapped
            .iter()
            .any(|b| b.id == EntityId((MAX_ECONOMY_BUILDINGS + 99) as u32))
    );
    assert!(!swapped.iter().any(|b| b.id == EntityId(0)));
    assert!(merge_buildings(&swapped, &[building(MAX_ECONOMY_BUILDINGS + 100)], &[]).is_none());
}
