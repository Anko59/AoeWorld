use super::review_tests::{client, contact_scene};
use super::*;
use aoe_core::{EntityId, PlayerId};
use aoe_core::{TileCoord, TileRect};
use aoe_protocol::{Age, BuildingKind, ConstructionState, GameplayBuildingState};
use wasm_bindgen_test::wasm_bindgen_test;

fn building(id: u32) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(id),
        player: PlayerId(1),
        age: Age::Dark,
        kind: BuildingKind::House,
        footprint: TileRect::from_xywh(282, 603, 2, 2),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 550,
        max_hp: 550,
        los: 4,
        food_remaining: None,
        research: None,
        training_queue: Vec::new(),
        last_hit: None,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
    }
}

#[wasm_bindgen_test]
async fn dense_scene_keeps_canonical_ids_across_shuffled_buildings_and_rubble() {
    let scene = contact_scene();
    let mut client = client(&scene).await;
    // Authoritative unit 1 sorts between the low building and the shuffled tail.
    client.village.buildings = (2..8192).rev().chain([0]).map(building).collect();
    client.village.rubble = vec![(building(8193), u64::MAX), (building(8192), u64::MAX)];
    let result = units(&client, &scene);
    assert_eq!(result.len(), 8194);
    for (id, unit) in result.iter().enumerate() {
        assert_eq!(unit.id, EntityId(id as u32));
        assert!(unit.elevation_meters.is_finite());
    }
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn interleaved_bridge_keeps_piece_order_in_the_sorted_scene() {
    let scene = contact_scene();
    let mut client = client(&scene).await;
    let mut bridge = building(2);
    bridge.kind = BuildingKind::Bridge;
    bridge.bridge_span =
        aoe_core::BridgeSpan::new(TileCoord::new(281, 603), TileCoord::new(284, 603));
    client.village.buildings = vec![building(4), bridge, building(0)];
    let result = units(&client, &scene);
    assert!(result.windows(2).all(|pair| pair[0].id <= pair[1].id));
    let pieces: Vec<_> = result.iter().filter(|u| u.id == EntityId(2)).collect();
    assert_eq!(pieces.len(), 4);
    for (offset, piece) in pieces.iter().enumerate() {
        assert_eq!(piece.position, [281.5 + offset as f64, 603.5]);
    }
    assert_eq!(result[0].id, EntityId(0));
    assert_eq!(result.last().unwrap().id, EntityId(4));
    client.canvas.remove();
}

#[wasm_bindgen_test]
async fn prepared_actor_culling_keeps_the_authoritative_and_selected_offscreen_units() {
    let mut scene = contact_scene();
    let mut client = client(&scene).await;
    let mut distant = *client.units.values().next().unwrap();
    distant.id = EntityId(9000);
    distant.position = aoe_core::WorldPosition::new(1000 * aoe_core::FIXED_SUBUNITS_PER_TILE, 0);
    client.units.insert(distant.id, distant);
    client.selected.units.push(distant.id);
    let mut far_building = building(3);
    far_building.footprint = TileRect::from_xywh(1000, 0, 2, 2);
    client.village.buildings = vec![building(2), far_building];
    scene.actor_extents = Some(client.art.scene_actor_extents());
    let result = units(&client, &scene);
    assert_eq!(
        result.iter().map(|u| u.id).collect::<Vec<_>>(),
        [EntityId(1), EntityId(2)]
    );
    assert!(client.units.contains_key(&distant.id));
    assert!(client.selected.units.contains(&distant.id));
    assert_eq!(client.village.buildings.len(), 2);
    client.canvas.remove();
}
