// Combined wire appends bridges before shelter commands, then navy and formations.
use super::*;

#[test]
fn garrison_and_selective_or_all_ungarrison_have_current_golden_wire() {
    for (order, golden) in [
        (
            ClientMessage::GarrisonOrder {
                sequence: 3,
                entity_id: EntityId(7),
                building_id: EntityId(9),
            },
            vec![25, 3, 7, 9],
        ),
        (
            ClientMessage::UngarrisonOrder {
                sequence: 4,
                building_id: EntityId(9),
                entity_id: None,
            },
            vec![26, 4, 9, 0],
        ),
        (
            ClientMessage::UngarrisonOrder {
                sequence: 5,
                building_id: EntityId(9),
                entity_id: Some(EntityId(7)),
            },
            vec![26, 5, 9, 1, 7],
        ),
    ] {
        let bytes = encode_client(&order).unwrap();
        assert_eq!(bytes, golden);
        assert_eq!(decode_client(&bytes).unwrap(), order);
    }
    let ack = ServerMessage::CommandAck {
        sequence: 3,
        result: CommandResult::RejectedGarrisonFull,
        applied_tick: Tick(4),
    };
    assert_eq!(encode_server(&ack).unwrap(), [3, 3, 34, 4]);
}

#[test]
fn vassal_training_and_specific_refusals_pin_the_current_wire() {
    let order = ClientMessage::TrainOrder {
        sequence: 3,
        building_id: EntityId(9),
        kind: UnitKind::Vassal,
    };
    let bytes = encode_client(&order).unwrap();
    assert_eq!(bytes, [5, 3, 9, 22]);
    assert_eq!(decode_client(&bytes).unwrap(), order);
    for (result, value) in [
        (CommandResult::RejectedVassalExists, 35),
        (CommandResult::RejectedVassalLeash, 36),
        (CommandResult::RejectedVassalHome, 37),
    ] {
        let ack = ServerMessage::CommandAck {
            sequence: 3,
            result,
            applied_tick: Tick(4),
        };
        assert_eq!(encode_server(&ack).unwrap(), [3, 3, value, 4]);
    }
    assert_eq!(MAX_POPULATION, 1_000);
}

#[test]
fn vassal_queue_and_garrison_roundtrip_and_other_buildings_reject_training() {
    let castle = GameplayBuildingState {
        id: EntityId(7),
        player: PlayerId(0),
        kind: BuildingKind::Castle,
        footprint: TileRect::from_xywh(1, 1, 4, 4),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 4800,
        max_hp: 4800,
        los: 11,
        food_remaining: None,
        training_queue: vec![TrainingState {
            kind: UnitKind::Vassal,
            progress_ticks: 1,
            total_ticks: 1200,
        }],
        research: None,
        last_hit: None,
        age: Age::Castle,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: vec![],
        bridge_span: None,
    };
    let message = |b| ServerMessage::Snapshot {
        revision: 1,
        tick: Tick(1),
        units: vec![],
        players: vec![],
        buildings: vec![b],
    };
    let snapshot = message(castle.clone());
    assert_eq!(
        decode_server(&encode_server(&snapshot).unwrap()).unwrap(),
        snapshot
    );
    let mut other = castle.clone();
    other.kind = BuildingKind::TownCenter;
    assert!(encode_server(&message(other)).is_err());
    let mut occupied = castle;
    occupied.training_queue.clear();
    occupied.garrison.push(GarrisonUnitState {
        id: EntityId(9),
        kind: UnitKind::Vassal,
        hp: 75,
        max_hp: 75,
    });
    let snapshot = message(occupied);
    assert_eq!(
        decode_server(&encode_server(&snapshot).unwrap()).unwrap(),
        snapshot
    );
}

#[test]
fn vassal_home_and_worker_management_are_current_owner_only_state() {
    let mut u = UnitState {
        id: EntityId(2),
        player: PlayerId(0),
        position: WorldPosition::new(1024, 2048),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::Vassal,
        task: UnitTask::Idle,
        carried: None,
        gathering: None,
        hp: 75,
        max_hp: 75,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        cargo_count: 0,
        home_castle: Some(EntityId(9)),
        managed_by: None,
    };
    // The current unit tail appends the two optional owner-visible identifiers.
    assert!(postcard::to_allocvec(&u).unwrap().ends_with(&[1, 9, 0]));
    assert_eq!(u.for_viewer(PlayerId(0)), u);
    assert_eq!(u.for_viewer(PlayerId(1)).home_castle, None);
    let message = |u| ServerMessage::Snapshot {
        revision: 1,
        tick: Tick(1),
        units: vec![u],
        players: vec![],
        buildings: vec![],
    };
    let snapshot = message(u);
    assert_eq!(
        decode_server(&encode_server(&snapshot).unwrap()).unwrap(),
        snapshot
    );
    u.kind = UnitKind::Villager;
    u.home_castle = None;
    u.hp = 25;
    u.max_hp = 25;
    u.managed_by = Some(EntityId(7));
    assert_eq!(u.for_viewer(PlayerId(1)).managed_by, None);
    let snapshot = message(u);
    assert_eq!(
        decode_server(&encode_server(&snapshot).unwrap()).unwrap(),
        snapshot
    );
    u.kind = UnitKind::Militia;
    assert!(
        encode_server(&message(u)).is_err(),
        "military cannot be managed by a Vassal"
    );
}
