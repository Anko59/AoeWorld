//! Combat wire state: hit points, attack target, last hit and the attack order.
use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use aoe_protocol::{
    BuildingKind, CommandResult, GameplayBuildingState, GameplayClientMessage, GameplayError,
    GameplayPlayerState, GameplayServerMessage, GameplayUnitState, HitState, Stockpile, UnitKind,
    UnitTask, decode_gameplay_client, decode_gameplay_server, encode_gameplay_client,
    encode_gameplay_server,
};

#[test]
fn patrol_order_pins_the_current_destination_wire() {
    let command = GameplayClientMessage::PatrolOrder {
        sequence: 3,
        entity_id: EntityId(7),
        destination: WorldPosition::new(10, 12),
    };
    let bytes = encode_gameplay_client(&command).unwrap();
    assert_eq!(bytes, [40, 3, 7, 20, 24]);
    assert_eq!(decode_gameplay_client(&bytes).unwrap(), command);
    assert!(decode_gameplay_client(&[40, 3, 7, 1, 24]).is_err());
}

fn unit(hp: u16, task: UnitTask, target: Option<EntityId>) -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(7),
        player: PlayerId(1),
        position: WorldPosition::new(2_048, 4_096),
        moving: false,
        planning: false,
        facing: 2,
        kind: UnitKind::Militia,
        task,
        carried: None,
        gathering: None,
        hp,
        max_hp: 40,
        attack_target: target,
        last_hit: Some(HitState {
            attacker: EntityId(3),
            tick: Tick(41),
        }),
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        sergeant: false,
        cargo_count: 0,
        native_action: None,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    }
}

#[test]
fn trebuchet_native_transition_is_bounded_to_a_live_stationary_trebuchet() {
    let at = Tick(220);
    let mut actor = unit(40, UnitTask::Attacking, Some(EntityId(6)));
    actor.kind = UnitKind::TrebuchetUnpacked;
    actor.native_action = Some(aoe_protocol::NativeAction::TrebuchetTransform(at));
    let message = |unit| GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(42),
        units: vec![unit],
        players: vec![],
        buildings: vec![],
    };
    let bytes = encode_gameplay_server(&message(actor)).unwrap();
    assert_eq!(&bytes[bytes.len() - 6..], &[1, 0, 0xdc, 0x01, 0, 0]);
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), message(actor));
    actor.moving = true;
    assert!(encode_gameplay_server(&message(actor)).is_err());
    actor.moving = false;
    actor.kind = UnitKind::Militia;
    assert!(encode_gameplay_server(&message(actor)).is_err());
}

fn center(hp: u16) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(1),
        player: PlayerId(0),
        kind: BuildingKind::TownCenter,
        footprint: TileRect::new(TileCoord::new(5, 5), TileCoord::new(9, 9)),
        training_queue: vec![],
        construction: aoe_protocol::ConstructionState::Complete,
        progress_permille: 1000,
        los: 8,
        food_remaining: None,
        research: None,
        hp,
        max_hp: 2400,
        last_hit: None,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
        age: aoe_protocol::Age::Dark,
    }
}

fn tick_with(
    units: Vec<GameplayUnitState>,
    buildings: Vec<GameplayBuildingState>,
) -> GameplayServerMessage {
    GameplayServerMessage::Tick {
        revision: 4,
        tick: Tick(42),
        changed_units: units,
        removals: vec![],
        changed_players: vec![],
        changed_buildings: buildings,
        building_removals: vec![],
    }
}

fn round_trips(message: &GameplayServerMessage) {
    let bytes = encode_gameplay_server(message).unwrap();
    assert_eq!(&decode_gameplay_server(&bytes).unwrap(), message);
}

#[test]
fn the_attack_order_and_its_rejections_round_trip() {
    let order = GameplayClientMessage::AttackOrder {
        sequence: 12,
        entity_id: EntityId(7),
        target_id: EntityId(9),
    };
    let bytes = encode_gameplay_client(&order).unwrap();
    assert_eq!(bytes, [7, 12, 7, 9]);
    assert_eq!(decode_gameplay_client(&bytes).unwrap(), order);

    let order = GameplayClientMessage::AttackSelectionOrder {
        sequence: 12,
        entity_ids: vec![EntityId(7), EntityId(11)],
        target_id: EntityId(9),
    };
    let bytes = encode_gameplay_client(&order).unwrap();
    assert_eq!(bytes, [42, 12, 2, 7, 11, 9]);
    assert_eq!(decode_gameplay_client(&bytes).unwrap(), order);
    for entity_ids in [
        vec![],
        vec![EntityId(7); 2],
        vec![EntityId(7); aoe_core::MAX_FORMATION_MEMBERS + 1],
    ] {
        assert!(
            encode_gameplay_client(&GameplayClientMessage::AttackSelectionOrder {
                sequence: 12,
                entity_ids,
                target_id: EntityId(9),
            })
            .is_err()
        );
    }
    for result in [
        CommandResult::RejectedInvalidTarget,
        CommandResult::RejectedInsufficientResources,
    ] {
        round_trips(&GameplayServerMessage::CommandAck {
            sequence: 12,
            result,
            applied_tick: Tick(5),
        });
    }
}

#[test]
fn death_and_hunting_fields_are_appended_to_the_current_unit_wire() {
    let mut dying = unit(0, UnitTask::Dying, None);
    dying.death_tick = Some(Tick(41));
    let bytes = encode_gameplay_server(&tick_with(vec![dying], vec![])).unwrap();
    // The unit tail includes the optional native action after the owner-only fields.
    assert_eq!(
        &bytes[bytes.len() - 17..],
        &[1, 41, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    round_trips(&tick_with(vec![dying], vec![]));
    dying.kind = UnitKind::Villager;
    dying.death_hunt = Some(aoe_protocol::AnimalKind::Sheep);
    let bytes = encode_gameplay_server(&tick_with(vec![dying], vec![])).unwrap();
    assert_eq!(
        &bytes[bytes.len() - 18..],
        &[1, 41, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    round_trips(&tick_with(vec![dying], vec![]));
    let mut hunter = unit(25, UnitTask::Hunting, None);
    hunter.kind = UnitKind::Villager;
    hunter.hunt_target = Some(EntityId(9));
    let bytes = encode_gameplay_server(&tick_with(vec![hunter], vec![])).unwrap();
    assert_eq!(
        &bytes[bytes.len() - 17..],
        &[0, 1, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    round_trips(&tick_with(vec![hunter], vec![]));
}

#[test]
fn combat_state_round_trips_for_units_and_buildings() {
    round_trips(&tick_with(
        vec![
            unit(31, UnitTask::Attacking, Some(EntityId(9))),
            unit(0, UnitTask::Dying, None),
        ],
        vec![center(2399)],
    ));
    let snapshot = GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(0),
        units: vec![unit(40, UnitTask::Idle, None)],
        players: vec![GameplayPlayerState {
            player: PlayerId(0),
            stockpile: Stockpile::default(),
            // A destroyed Town Center leaves population above a zero cap.
            population_used: 3,
            population_cap: 0,
            age: aoe_protocol::Age::Dark,
            researched: 0,
            recent_deliveries: [0; 4],
            unit_counts: [0; aoe_protocol::UnitKind::ALL.len()],
            held_tiles: 0,
            held_share_ppm: 0,
            market_prices: aoe_protocol::MarketPrices::default(),
        }],
        buildings: vec![center(1)],
    };
    round_trips(&snapshot);
}

#[test]
fn inconsistent_combat_state_is_refused() {
    let refused = |message: GameplayServerMessage| {
        assert!(matches!(
            encode_gameplay_server(&message),
            Err(GameplayError::InvalidEconomyState)
        ));
    };
    refused(tick_with(vec![unit(41, UnitTask::Idle, None)], vec![]));
    refused(tick_with(vec![unit(0, UnitTask::Idle, None)], vec![]));
    refused(tick_with(vec![unit(10, UnitTask::Dying, None)], vec![]));
    refused(tick_with(
        vec![unit(0, UnitTask::Dying, Some(EntityId(9)))],
        vec![],
    ));
    refused(tick_with(vec![], vec![center(0)]));
    refused(tick_with(vec![], vec![center(2401)]));
    let mut no_health = unit(0, UnitTask::Dying, None);
    no_health.max_hp = 0;
    refused(tick_with(vec![no_health], vec![]));
}

#[test]
fn projectile_view_round_trips_more_than_four_thousand_live_shots() {
    use aoe_protocol::{ProjectileKind, ProjectileState};
    let at = WorldPosition::new(2048, 2048);
    let projectiles = (1..=5_000)
        .map(|id| ProjectileState {
            id,
            attacker: Some(EntityId(1)),
            from: at,
            to: at,
            launched: Tick(40),
            impact: Tick(55),
            kind: ProjectileKind::Arrow,
        })
        .collect();
    round_trips(&GameplayServerMessage::Projectiles {
        revision: 1,
        tick: Tick(42),
        projectiles,
    });
}
