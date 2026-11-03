//! A server-built message with one invalid entity degrades to the valid rest,
//! and the violation names the rule and the entity. Encoding stays strict.
use aoe_core::{EntityId, PlayerId, Tick, TileCoord, TileRect, WorldPosition};
use aoe_protocol::*;

fn unit(id: u32) -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(id),
        player: PlayerId(0),
        position: WorldPosition::new(1024, 1024),
        moving: false,
        planning: false,
        facing: 3,
        kind: UnitKind::Villager,
        task: UnitTask::Idle,
        gathering: None,
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
        carried: None,
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
fn building(id: u32) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(id),
        player: PlayerId(0),
        kind: BuildingKind::TownCenter,
        footprint: TileRect::new(TileCoord::new(5, 5), TileCoord::new(9, 9)),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 2400,
        max_hp: 2400,
        los: 8,
        food_remaining: None,
        research: None,
        training_queue: Vec::new(),
        last_hit: None,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
        age: Age::Dark,
    }
}
fn snapshot(
    units: Vec<GameplayUnitState>,
    buildings: Vec<GameplayBuildingState>,
) -> GameplayServerMessage {
    GameplayServerMessage::Snapshot {
        revision: 1,
        tick: Tick(3),
        units,
        players: vec![],
        buildings,
    }
}
fn parts(message: &GameplayServerMessage) -> (&[GameplayUnitState], &[GameplayBuildingState]) {
    match message {
        GameplayServerMessage::Snapshot {
            units, buildings, ..
        } => (units, buildings),
        _ => unreachable!(),
    }
}

#[test]
fn a_unit_that_breaks_a_rule_is_named_repaired_and_the_rest_arrive() {
    type Break = fn(&mut GameplayUnitState);
    let cases: Vec<(&str, Break)> = vec![
        ("unit.managed_by", |u| {
            u.hp = 0;
            u.task = UnitTask::Dying;
            u.managed_by = Some(EntityId(9));
        }),
        ("unit.commanded_by", |u| u.commanded_by = Some(EntityId(9))),
        ("unit.general", |u| {
            u.kind = UnitKind::General;
            u.general = Some(GeneralStatus {
                army_size: 101,
                state: GeneralState::Idle,
                training: false,
                queued: 0,
                mix: [0; 5],
                counts: [0; 5],
            });
        }),
        ("unit.facing", |u| u.facing = 8),
        ("unit.hp_at_most_max_hp", |u| u.hp = 26),
        ("unit.death_tick_only_when_dead", |u| {
            u.death_tick = Some(Tick(1))
        }),
        ("unit.cargo_count", |u| u.cargo_count = 1),
    ];
    for (rule, break_it) in cases {
        let mut bad = unit(2);
        break_it(&mut bad);
        let mut message = snapshot(vec![unit(1), bad, unit(3)], vec![building(10)]);
        assert!(encode_gameplay_server(&message).is_err(), "{rule}");
        let violations = gameplay_server_violations(&message);
        assert_eq!(violations.len(), 1, "{rule}: {violations:?}");
        assert_eq!(violations[0].rule, rule);
        assert_eq!(violations[0].entity, Some(EntityId(2)));
        let reported = sanitize_gameplay_server(&mut message);
        assert_eq!(reported[0].rule, rule);
        assert!(reported[0].detail.contains("repaired"), "{rule}");
        encode_gameplay_server(&message).unwrap_or_else(|e| panic!("{rule}: {e}"));
        assert_eq!(parts(&message).0.len(), 3, "{rule}");
        assert_eq!(parts(&message).1.len(), 1);
    }
}

#[test]
fn an_unrepairable_unit_or_building_is_omitted_and_the_others_arrive() {
    let mut dead_without_task = unit(2);
    dead_without_task.hp = 0;
    let mut destroyed = building(11);
    destroyed.hp = 0;
    let mut wrong_size = building(12);
    wrong_size.footprint = TileRect::new(TileCoord::new(1, 1), TileCoord::new(2, 2));
    let mut message = snapshot(
        vec![unit(1), dead_without_task, unit(3)],
        vec![building(10), destroyed, wrong_size, building(13)],
    );
    let rules: Vec<_> = gameplay_server_violations(&message)
        .iter()
        .map(|v| (v.rule, v.entity.map(|id| id.0)))
        .collect();
    assert_eq!(
        rules,
        [
            ("unit.dying_iff_no_hp", Some(2)),
            ("building.hp_positive", Some(11)),
            ("building.footprint_matches_kind", Some(12)),
        ]
    );
    assert_eq!(sanitize_gameplay_server(&mut message).len(), 3);
    encode_gameplay_server(&message).unwrap();
    let (units, buildings) = parts(&message);
    assert_eq!(units.iter().map(|u| u.id.0).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(
        buildings.iter().map(|b| b.id.0).collect::<Vec<_>>(),
        [10, 13]
    );
}

#[test]
fn building_fields_with_a_safe_repair_are_cleared_not_dropped() {
    let mut open_gate = building(10);
    open_gate.gate_open = true;
    let mut over = building(11);
    over.hp = 2500;
    let mut message = snapshot(vec![], vec![open_gate, over]);
    sanitize_gameplay_server(&mut message);
    encode_gameplay_server(&message).unwrap();
    let (_, buildings) = parts(&message);
    assert_eq!(buildings.len(), 2);
    assert!(!buildings[0].gate_open);
    assert_eq!(buildings[1].hp, 2400);
}

#[test]
fn whole_message_rules_are_reported_but_not_repaired() {
    let mut message = GameplayServerMessage::Projectiles {
        revision: 0,
        tick: Tick(0),
        projectiles: vec![],
    };
    assert!(sanitize_gameplay_server(&mut message).is_empty());
    assert!(encode_gameplay_server(&message).is_err());
    assert_eq!(
        gameplay_server_violations(&message)[0].rule,
        "revision_nonzero"
    );
}

#[test]
fn one_bad_garrison_occupant_costs_only_itself() {
    let occupant = |id: u32, hp: u16| GarrisonUnitState {
        id: EntityId(id),
        kind: UnitKind::Villager,
        hp,
        max_hp: 25,
        vassal: None,
    };
    let mut tc = building(10);
    tc.garrison = vec![occupant(1, 25), occupant(2, 0), occupant(3, 25)];
    let mut message = snapshot(vec![], vec![tc]);
    let reported = sanitize_gameplay_server(&mut message);
    assert_eq!(reported[0].rule, "building.garrison");
    encode_gameplay_server(&message).unwrap();
    let ids: Vec<_> = parts(&message).1[0]
        .garrison
        .iter()
        .map(|g| g.id.0)
        .collect();
    assert_eq!(ids, [1, 3]);
}

#[test]
fn a_vassal_report_on_a_non_vassal_is_cleared_not_the_building() {
    let report = VassalReport {
        managed: 3,
        residents: 4,
        wait: VassalWait::Growing,
    };
    let mut tc = building(10);
    tc.garrison = vec![GarrisonUnitState {
        id: EntityId(1),
        kind: UnitKind::Villager,
        hp: 25,
        max_hp: 25,
        vassal: Some(report),
    }];
    let mut message = snapshot(vec![], vec![tc]);
    let reported = sanitize_gameplay_server(&mut message);
    assert_eq!(reported[0].rule, "building.garrison");
    assert!(reported[0].detail.starts_with("repaired"));
    encode_gameplay_server(&message).unwrap();
    let garrison = &parts(&message).1[0].garrison;
    assert_eq!(garrison.len(), 1);
    assert!(garrison[0].vassal.is_none());
}
