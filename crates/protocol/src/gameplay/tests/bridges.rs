// Combined building roster puts Bridge after the four Castle building kinds.
use super::*;

#[test]
fn bridge_snapshots_accept_only_bounded_single_lane_footprints() {
    let mut building = GameplayBuildingState {
        id: EntityId(7),
        player: PlayerId(0),
        kind: BuildingKind::Bridge,
        footprint: TileRect::from_xywh(1, 1, 3, 1),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 1200,
        max_hp: 1200,
        los: 4,
        food_remaining: None,
        training_queue: vec![],
        research: None,
        last_hit: None,
        age: Age::Feudal,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
    };
    for (width, height, valid) in [
        (3, 1, true),
        (42, 1, true),
        (1, 3, true),
        (1, 42, true),
        (1, 1, false),
        (2, 1, false),
        (43, 1, false),
        (2, 2, false),
        (1, 43, false),
    ] {
        building.footprint = TileRect::from_xywh(1, 1, width, height);
        let message = ServerMessage::Snapshot {
            revision: 1,
            tick: Tick(1),
            units: vec![],
            players: vec![],
            buildings: vec![building.clone()],
        };
        if valid {
            let bytes = encode_server(&message).unwrap();
            assert_eq!(decode_server(&bytes).unwrap(), message);
        } else {
            assert!(encode_server(&message).is_err());
        }
    }
}

#[test]
fn bridge_order_has_bounded_builders_and_current_golden_wire() {
    let order = ClientMessage::BuildBridgeOrder {
        sequence: 3,
        builder_ids: vec![EntityId(7)],
        from: TileCoord::new(29, 20),
        to: TileCoord::new(34, 20),
        queued: true,
    };
    let bytes = encode_client(&order).unwrap();
    // The current wire appends Shift queue intent so bridge orders preserve construction work.
    assert_eq!(bytes, [24, 3, 1, 7, 58, 40, 68, 40, 1]);
    assert_eq!(decode_client(&bytes).unwrap(), order);
    assert_eq!(postcard::to_allocvec(&BuildingKind::Bridge).unwrap(), [20]);
    let too_many = ClientMessage::BuildBridgeOrder {
        sequence: 1,
        builder_ids: vec![EntityId(7); MAX_BUILD_GROUP + 1],
        from: TileCoord::new(1, 1),
        to: TileCoord::new(4, 1),
        queued: false,
    };
    assert!(decode_client(&encode_client(&too_many).unwrap()).is_err());
}
