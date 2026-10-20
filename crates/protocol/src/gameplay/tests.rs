use super::*;
use aoe_core::{Seed, TileCoord, WorldConfig};

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
