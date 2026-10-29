use super::*;
use aoe_core::{Seed, TileCoord, WorldConfig};

mod bridges;
mod fog;
mod formations;
mod garrison;
mod queues;

#[test]
fn building_orders_pin_the_current_queue_flag() {
    // Shift placement adds one boolean to each current construction order.
    for queued in [false, true] {
        let building = ClientMessage::BuildOrder {
            sequence: 3,
            builder_ids: vec![EntityId(7)],
            kind: BuildingKind::House,
            origin: TileCoord::new(10, 12),
            queued,
        };
        let wall = ClientMessage::BuildWallOrder {
            sequence: 3,
            builder_ids: vec![EntityId(7)],
            from: TileCoord::new(10, 12),
            to: TileCoord::new(11, 12),
            queued,
        };
        for (order, expected) in [
            (building, vec![8, 3, 1, 7, 1, 20, 24, u8::from(queued)]),
            (wall, vec![9, 3, 1, 7, 20, 24, 22, 24, u8::from(queued)]),
        ] {
            let bytes = encode_client(&order).unwrap();
            assert_eq!(bytes, expected);
            assert_eq!(decode_client(&bytes).unwrap(), order);
        }
    }
}

#[test]
fn auto_scout_orders_have_current_golden_wire() {
    // QueueMove occupies the preceding discriminant in the combined protocol;
    // regenerate the current order bytes for this intentional enum change.
    for enabled in [false, true] {
        let order = ClientMessage::AutoScoutOrder {
            sequence: 3,
            entity_id: EntityId(7),
            enabled,
        };
        let bytes = encode_client(&order).unwrap();
        assert_eq!(bytes, [23, 3, 7, u8::from(enabled)]);
        assert_eq!(decode_client(&bytes).unwrap(), order);
    }
    assert!(decode_client(&[23, 3, 7, 2]).is_err());
}

#[test]
fn farm_reseed_orders_have_current_golden_wire() {
    // The combined protocol inserts QueueMove before the eco orders.
    for add in [false, true] {
        let order = ClientMessage::FarmReseedOrder {
            sequence: 3,
            building_id: EntityId(7),
            add,
        };
        let bytes = encode_client(&order).unwrap();
        assert_eq!(bytes, [22, 3, 7, u8::from(add)]);
        assert_eq!(decode_client(&bytes).unwrap(), order);
    }
    assert!(decode_client(&[22, 3, 7, 2]).is_err());
}

#[test]
fn unexplored_placement_rejection_has_current_golden_wire() {
    let ack = ServerMessage::CommandAck {
        sequence: 3,
        result: CommandResult::RejectedUnexploredPlacement,
        applied_tick: Tick(4),
    };
    let bytes = encode_server(&ack).unwrap();
    assert_eq!(bytes, [3, 3, 32, 4]);
    assert_eq!(decode_server(&bytes).unwrap(), ack);
}

#[test]
fn fishing_appends_wire_discriminants_and_round_trips_orders() {
    assert_eq!(postcard::to_allocvec(&UnitKind::FishingShip).unwrap(), [3]);
    assert_eq!(postcard::to_allocvec(&BuildingKind::Dock).unwrap(), [9]);
    assert_eq!(postcard::to_allocvec(&UnitTask::Fishing).unwrap(), [11]);
    let order = ClientMessage::TrainOrder {
        sequence: 1,
        building_id: EntityId(2),
        kind: UnitKind::FishingShip,
    };
    let bytes = encode_client(&order).unwrap();
    assert_eq!(bytes, [5, 1, 2, 3]);
    assert_eq!(decode_client(&bytes).unwrap(), order);
}

fn region() -> TileRect {
    TileRect::new(TileCoord::new(0, 0), TileCoord::new(8, 8))
}

#[test]
fn gameplay_messages_round_trip_independently_of_diagnostic_protocol() {
    let message = ClientMessage::MoveOrder {
        sequence: 9,
        entity_id: EntityId(3),
        destination: WorldPosition::new(42, 84),
    };
    assert_eq!(
        decode_client(&encode_client(&message).unwrap()).unwrap(),
        message
    );
    let welcome = ServerMessage::Welcome {
        version: VERSION,
        world_id: 11,
        map_content_hash: None,
        map_metadata: None,
        width_tiles: WorldConfig::new(16_384, 16_384, Seed(1))
            .unwrap()
            .width_tiles,
        height_tiles: 16_384,
        coordinate_precision: 1_024,
        tick_hz: 20,
        role: Role::Spectator,
        primary_unit_id: EntityId(0),
        resume_token: None,
    };
    assert_eq!(
        decode_server(&encode_server(&welcome).unwrap()).unwrap(),
        welcome
    );
    let reset = ServerMessage::WorldReset { world_id: 12 };
    assert_eq!(
        decode_server(&encode_server(&reset).unwrap()).unwrap(),
        reset
    );
    let acknowledgment = ServerMessage::CommandAck {
        sequence: 7,
        result: CommandResult::RejectedPathBudgetExceeded,
        applied_tick: Tick(3),
    };
    assert_eq!(
        decode_server(&encode_server(&acknowledgment).unwrap()).unwrap(),
        acknowledgment
    );
}

#[test]
fn gameplay_collections_are_bounded() {
    let too_many = vec![
        UnitState {
            id: EntityId(1),
            player: PlayerId(0),
            position: WorldPosition::new(0, 0),
            moving: false,
            planning: false,
            facing: 0,
            kind: UnitKind::Scout,
            task: UnitTask::Idle,
            carried: None,
            gathering: None,
            hp: 25,
            max_hp: 25,
            attack_target: None,
            last_hit: None,
            death_tick: None,
            hunt_target: None,
            death_hunt: None,
            auto_scout: false,
            cargo_count: 0,
            home_castle: None,
            managed_by: None,
            commanded_by: None,
            general: None,
            stance: Default::default(),
        };
        MAX_SUBSCRIBED_UNITS + 1
    ];
    assert!(matches!(
        encode_server(&ServerMessage::Snapshot {
            revision: 1,
            tick: Tick(0),
            units: too_many,
            players: Vec::new(),
            buildings: Vec::new(),
        }),
        Err(Error::TooManyUnits)
    ));
    assert!(matches!(
        decode_client(&vec![0; MAX_MESSAGE + 1]),
        Err(Error::TooLarge)
    ));
    let _ = region();
}

mod slots;

#[test]
fn rally_order_has_one_current_golden_wire_format() {
    let message = ClientMessage::SetRallyOrder {
        sequence: 9,
        building_id: EntityId(3),
        target: RallyTarget::Resource(77),
    };
    let bytes = encode_client(&message).unwrap();
    assert_eq!(bytes, [20, 9, 3, 1, 77]);
    assert_eq!(decode_client(&bytes).unwrap(), message);
}
