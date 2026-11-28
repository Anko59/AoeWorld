use super::*;

#[wasm_bindgen_test]
async fn remembered_building_art_uses_last_seen_age_instead_of_live_owner_age() {
    use aoe_core::{TileCoord, TileRect};
    use aoe_protocol::GameplayPlayerState as PlayerReport;
    use aoe_protocol::{BuildingKind, ConstructionState, GameplayBuildingState};
    let scene = contact_scene();
    let mut client = client(&scene).await;
    client.village.players.push(PlayerReport {
        player: PlayerId(2),
        stockpile: Default::default(),
        population_used: 0,
        population_cap: 5,
        age: aoe_protocol::Age::Feudal,
        researched: aoe_protocol::Tech::FeudalAge.bit(),
        recent_deliveries: [0; 4],
        unit_counts: [0; aoe_protocol::UnitKind::ALL.len()],
        held_tiles: 0,
        held_share_ppm: 0,
        food_upkeep_per_minute: 0,
        food_upkeep_shortage: false,
        market_prices: Default::default(),
    });
    client.village.buildings.push(GameplayBuildingState {
        id: EntityId(2),
        player: PlayerId(2),
        kind: BuildingKind::House,
        footprint: TileRect::new(TileCoord::new(282, 603), TileCoord::new(284, 605)),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 550,
        max_hp: 550,
        los: 4,
        food_remaining: None,
        training_queue: vec![],
        research: None,
        last_hit: None,
        age: aoe_protocol::Age::Dark,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
    });
    let rendered = |client: &Client| {
        units(client, &scene)
            .into_iter()
            .find(|u| u.id == EntityId(2))
            .unwrap()
            .kind
    };
    assert_eq!(rendered(&client), aoe_rendering::SceneUnitKind::House);
    client.village.buildings[0].age = aoe_protocol::Age::Feudal;
    assert_eq!(rendered(&client), aoe_rendering::SceneUnitKind::FeudalHouse);
}
