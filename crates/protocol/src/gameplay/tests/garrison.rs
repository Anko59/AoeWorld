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
    assert_eq!(encode_server(&ack).unwrap(), [3, 3, 33, 4]);
}

#[test]
fn steward_training_and_specific_refusals_pin_the_current_wire() {
    let order = ClientMessage::TrainOrder {
        sequence: 3,
        building_id: EntityId(9),
        kind: UnitKind::Steward,
    };
    let bytes = encode_client(&order).unwrap();
    assert_eq!(bytes, [5, 3, 9, 34]);
    assert_eq!(postcard::to_allocvec(&UnitKind::Vassal).unwrap(), [22]);
    assert_eq!(postcard::to_allocvec(&UnitKind::Onager).unwrap(), [33]);
    assert_eq!(decode_client(&bytes).unwrap(), order);
    for (result, value) in [
        (CommandResult::RejectedStewardExists, 34),
        (CommandResult::RejectedStewardLeash, 35),
        (CommandResult::RejectedStewardHome, 36),
        (CommandResult::RejectedRequiresImperialAge, 37),
        (CommandResult::RejectedPopulationCap, 38),
        (CommandResult::RejectedInvalidStewardSettings, 39),
    ] {
        let ack = ServerMessage::CommandAck {
            sequence: 3,
            result,
            applied_tick: Tick(4),
        };
        assert_eq!(encode_server(&ack).unwrap(), [3, 3, value, 4]);
    }
    assert_eq!(MAX_SUBSCRIBED_UNITS, 65_536);
}

#[test]
fn steward_queue_and_garrison_roundtrip_and_other_buildings_reject_training() {
    let castle = GameplayBuildingState {
        id: EntityId(7),
        player: PlayerId(0),
        kind: BuildingKind::TownCenter,
        footprint: TileRect::from_xywh(1, 1, 4, 4),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 4800,
        max_hp: 4800,
        los: 11,
        food_remaining: None,
        training_queue: vec![TrainingState {
            kind: UnitKind::Steward,
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
    // Town Centers train Stewards and accept rallies through the same command path.
    // Every subsequent snapshot must remain encodable after that right-click.
    let mut castle = castle;
    let position = WorldPosition::new(4096, 4096);
    castle.rally_point = Some(RallyPoint {
        target: RallyTarget::Ground(position),
        position,
    });
    let snapshot = message(castle.clone());
    assert_eq!(
        decode_server(&encode_server(&snapshot).unwrap()).unwrap(),
        snapshot
    );
    let mut other = castle.clone();
    other.kind = BuildingKind::Castle;
    assert!(encode_server(&message(other)).is_err());
    let mut occupied = castle;
    occupied.training_queue.clear();
    occupied.garrison.push(GarrisonUnitState {
        id: EntityId(9),
        kind: UnitKind::Steward,
        hp: 75,
        max_hp: 75,
        vassal: None,
        steward: Some(crate::StewardReport {
            weights: [45, 30, 15, 10],
            cap: 75,
            training: true,
            managed: 20,
            residents: 31,
            wait: crate::StewardWait::Food,
        }),
    });
    assert_eq!(
        postcard::to_allocvec(&occupied.garrison[0].steward.unwrap()).unwrap(),
        [45, 30, 15, 10, 75, 1, 20, 31, 4]
    );
    let snapshot = message(occupied.clone());
    assert_eq!(
        decode_server(&encode_server(&snapshot).unwrap()).unwrap(),
        snapshot
    );
    occupied.garrison[0].steward.as_mut().unwrap().weights = [0; 4];
    assert!(encode_server(&message(occupied.clone())).is_err());
    occupied.garrison[0].steward.as_mut().unwrap().weights = [45, 30, 15, 10];
    occupied.garrison[0].steward.as_mut().unwrap().cap = 101;
    assert!(encode_server(&message(occupied)).is_err());
}

#[test]
fn roaming_steward_and_worker_management_are_current_owner_only_state() {
    let mut u = UnitState {
        id: EntityId(2),
        player: PlayerId(0),
        position: WorldPosition::new(1024, 2048),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::Steward,
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
        sergeant: false,
        cargo_count: 0,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    };
    // The current unit tail appends the two optional owner-visible identifiers.
    assert!(
        postcard::to_allocvec(&u)
            .unwrap()
            .ends_with(&[0, 0, 0, 0, 0])
    );
    u.stance = CombatStance::Defensive;
    assert_eq!(u.for_viewer(PlayerId(0)), u);
    assert_eq!(u.for_viewer(PlayerId(1)).home_castle, None);
    assert_eq!(u.for_viewer(PlayerId(1)).stance, CombatStance::Aggressive);
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
    let mut bound = u;
    bound.home_castle = Some(EntityId(9));
    assert!(encode_server(&message(bound)).is_err());
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
    u.kind = UnitKind::FishingShip;
    let snapshot = message(u);
    assert_eq!(
        decode_server(&encode_server(&snapshot).unwrap()).unwrap(),
        snapshot
    );
    assert_eq!(u.for_viewer(PlayerId(1)).managed_by, None);
    u.kind = UnitKind::Militia;
    assert!(
        encode_server(&message(u)).is_err(),
        "military cannot be managed by a Steward"
    );
}

#[test]
fn general_training_pins_current_wire_identity() {
    let m = ClientMessage::TrainOrder {
        sequence: 7,
        building_id: EntityId(9),
        kind: UnitKind::General,
    };
    let encoded = encode_client(&m).unwrap();
    assert_eq!(encoded, vec![5, 7, 9, 23]);
    assert_eq!(decode_client(&encoded).unwrap(), m);
}

#[test]
fn general_summary_and_command_tags_are_bounded_owner_only_state() {
    let mut u = UnitState {
        id: EntityId(2),
        player: PlayerId(0),
        position: WorldPosition::new(1024, 2048),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::General,
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
        sergeant: false,
        cargo_count: 0,
        home_castle: Some(EntityId(9)),
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    };

    u.general = Some(crate::GeneralStatus {
        army_size: 200,
        state: crate::GeneralState::Retreating,
        training: true,
        queued: 3,
        mix: [35, 30, 20, 10, 5],
        counts: [200, 0, 0, 0, 0],
        squads: std::array::from_fn(|n| {
            Some(crate::SquadStatus {
                leader: EntityId(100 + n as u32),
                kind: crate::SquadKind::Infantry,
                size: 20,
                state: crate::GeneralState::Retreating,
            })
        }),
    });
    assert_eq!(u.for_viewer(PlayerId(0)), u);
    let foreign = u.for_viewer(PlayerId(1));
    assert_eq!(foreign.general, None);
    assert_eq!(foreign.commanded_by, None);
    assert_eq!(foreign.home_castle, None);
    let message = |u| ServerMessage::Snapshot {
        revision: 1,
        tick: Tick(1),
        units: vec![u],
        players: vec![],
        buildings: vec![],
    };
    let bytes = encode_server(&message(u)).unwrap();
    let valid = u.general;
    u.general.as_mut().unwrap().squads[0].as_mut().unwrap().size = 21;
    assert!(
        encode_server(&message(u)).is_err(),
        "bounded typed capacity"
    );
    u.general = valid;
    u.general.as_mut().unwrap().squads[1] = u.general.unwrap().squads[0];
    assert!(encode_server(&message(u)).is_err(), "distinct leaders");
    u.general = valid;
    u.general.as_mut().unwrap().squads[0] = None;
    assert!(
        encode_server(&message(u)).is_err(),
        "the roster accounts for every soldier"
    );
    u.general = valid;
    // Current wire: state, training, queue, mix, counts and ten typed offices.
    let roster: Vec<u8> = (100..110).flat_map(|id| [1, id, 0, 20, 3]).collect();
    let mut tail = vec![
        0, 0, 1, 200, 1, 3, 1, 3, 35, 30, 20, 10, 5, 200, 1, 0, 0, 0, 0,
    ];
    tail.extend_from_slice(&roster);
    tail.extend_from_slice(&[0, 0]);
    assert_eq!(&bytes[bytes.len() - tail.len()..], tail);
    assert_eq!(decode_server(&bytes).unwrap(), message(u));
    u.general.as_mut().unwrap().state = crate::GeneralState::Idle;
    let idle = encode_server(&message(u)).unwrap();
    tail[5] = 0;
    assert_eq!(&idle[idle.len() - tail.len()..], tail);
    assert_eq!(decode_server(&idle).unwrap(), message(u));
    u.general.as_mut().unwrap().army_size = 201;
    assert!(encode_server(&message(u)).is_err());
    u.general = None;
    u.home_castle = None;
    u.kind = UnitKind::Steward;
    u.commanded_by = Some(EntityId(88));
    assert!(
        encode_server(&message(u)).is_err(),
        "a Steward cannot be a member of a General's army"
    );
    u.kind = UnitKind::Knight;
    u.commanded_by = Some(EntityId(88));
    assert!(encode_server(&message(u)).is_ok());
    assert_eq!(u.for_viewer(PlayerId(1)).commanded_by, None);
    u.commanded_by = Some(u.id);
    assert!(encode_server(&message(u)).is_err());
}

#[test]
fn steward_settings_order_pins_weights_cap_and_training_wire() {
    let order = ClientMessage::StewardSettingsOrder {
        sequence: 3,
        steward_id: EntityId(9),
        weights: [45, 30, 15, 10],
        cap: 100,
        training: true,
    };
    let bytes = encode_client(&order).unwrap();
    assert_eq!(bytes, [33, 3, 9, 45, 30, 15, 10, 100, 1]);
    assert_eq!(decode_client(&bytes).unwrap(), order);
}

#[test]
fn dismiss_steward_order_pins_the_current_wire() {
    let order = ClientMessage::StewardDismissOrder {
        sequence: 4,
        steward_id: EntityId(9),
    };
    let bytes = encode_client(&order).unwrap();
    assert_eq!(bytes, [34, 4, 9]);
    assert_eq!(decode_client(&bytes).unwrap(), order);
}

#[test]
fn vassal_development_order_appends_the_wire_discriminant() {
    let order = ClientMessage::VassalDevelopmentOrder {
        sequence: 1,
        vassal_id: EntityId(2),
        enabled: false,
    };
    let wire = encode_client(&order).unwrap();
    assert_eq!(wire, [35, 1, 2, 0]);
    assert_eq!(decode_client(&wire).unwrap(), order);
}

#[test]
fn vassal_report_has_current_golden_wire_and_bounded_counts() {
    let report = crate::VassalReport {
        villagers: 20,
        military: 20,
        development: true,
        developed: false,
        alert: true,
        next: Some(BuildingKind::WatchTower),
        wait: 0,
        completed: 1,
        total: 77,
        responding: 1,
    };
    assert_eq!(
        postcard::to_allocvec(&report).unwrap(),
        [20, 20, 1, 0, 1, 1, 14, 0, 1, 77, 1]
    );
    let mut b = GameplayBuildingState {
        id: EntityId(2),
        player: PlayerId(0),
        kind: BuildingKind::Castle,
        footprint: TileRect::from_xywh(10, 10, 4, 4),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 4800,
        max_hp: 4800,
        los: 11,
        food_remaining: None,
        training_queue: vec![],
        research: None,
        last_hit: None,
        age: Age::Castle,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: vec![],
        bridge_span: None,
    };
    b.kind = BuildingKind::Castle;
    b.garrison = vec![GarrisonUnitState {
        id: EntityId(99),
        kind: UnitKind::Vassal,
        hp: 75,
        max_hp: 75,
        steward: None,
        vassal: Some(report),
    }];
    let msg = |b| ServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![],
        buildings: vec![b],
        players: vec![],
    };
    assert!(encode_server(&msg(b.clone())).is_ok());
    b.garrison[0].vassal.as_mut().unwrap().villagers = 21;
    assert!(encode_server(&msg(b)).is_err());
}
