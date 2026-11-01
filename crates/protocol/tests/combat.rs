//! Combat wire state: hit points, attack target, last hit and the attack order.
use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use aoe_protocol::{
    BuildingKind, CommandResult, GameplayBuildingState, GameplayClientMessage, GameplayError,
    GameplayPlayerState, GameplayServerMessage, GameplayUnitState, HitState, Stockpile, UnitKind,
    UnitTask, decode_gameplay_client, decode_gameplay_server, encode_gameplay_client,
    encode_gameplay_server,
};

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
        cargo_count: 0,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    }
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
    assert_eq!(
        decode_gameplay_client(&encode_gameplay_client(&order).unwrap()).unwrap(),
        order
    );
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
    // Current unit tails append home_castle, managed_by and combat stance.
    // Regenerate the suffix pins for those owner-only fields, preserving option tags.
    assert_eq!(
        &bytes[bytes.len() - 15..],
        &[1, 41, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    round_trips(&tick_with(vec![dying], vec![]));
    dying.kind = UnitKind::Villager;
    dying.death_hunt = Some(aoe_protocol::AnimalKind::Sheep);
    let bytes = encode_gameplay_server(&tick_with(vec![dying], vec![])).unwrap();
    assert_eq!(
        &bytes[bytes.len() - 16..],
        &[1, 41, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    round_trips(&tick_with(vec![dying], vec![]));
    let mut hunter = unit(25, UnitTask::Hunting, None);
    hunter.kind = UnitKind::Villager;
    hunter.hunt_target = Some(EntityId(9));
    let bytes = encode_gameplay_server(&tick_with(vec![hunter], vec![])).unwrap();
    assert_eq!(
        &bytes[bytes.len() - 15..],
        &[0, 1, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
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
            age: aoe_protocol::Age::Dark,
            researched: 0,
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
