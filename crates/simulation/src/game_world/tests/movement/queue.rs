use super::*;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn restored_general_keeps_destinations_queued_behind_his_army_formation() {
    let (mut saved, start) = super::snapshot_roundtrip::flat();
    let point = |dx, dy| at(start.x + dx, start.y + dy);
    saved
        .spawn_kind(PlayerId(0), UnitKind::Militia, point(6, 7))
        .unwrap();
    let general = saved
        .spawn_kind(PlayerId(0), UnitKind::General, point(6, 6))
        .unwrap();
    saved.issue_move(general, point(40, 6)).unwrap();
    saved.issue_queued_move(general, point(40, 20)).unwrap();
    for _ in 0..20 {
        saved.advance();
    }
    assert!(saved.formation(general).is_some());
    let expected = saved.move_destinations(general);
    assert_eq!(expected.last(), Some(&point(40, 20)));
    let snapshot = saved.snapshot([0; 32]).unwrap();
    let (mut restored, _) = super::snapshot_roundtrip::flat();
    let report = restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    assert_eq!(report.units_lost, 0);
    assert_eq!(restored.move_destinations(general), expected);
}

#[test]
fn queued_player_moves_visit_each_destination_and_replay_identically() {
    let mut first = world();
    let mut second = world();
    let a = first.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    let b = second.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    let points = [at(14, 10), at(14, 14), at(10, 14), at(10, 10)];
    for (world, id) in [(&mut first, a), (&mut second, b)] {
        world.issue_move(id, points[0]).unwrap();
        for p in &points[1..] {
            world.issue_queued_move(id, *p).unwrap();
        }
        assert_eq!(world.move_destinations(id), points);
    }
    let mut visited = 0;
    for _ in 0..300 {
        first.advance();
        second.advance();
        assert_eq!(first.canonical_hash(), second.canonical_hash());
        if visited < points.len() && first.unit(a).unwrap().position == points[visited] {
            visited += 1;
        }
    }
    assert_eq!(visited, points.len());
    assert!(first.move_destinations(a).is_empty());
    assert_eq!(first.active_mover_count(), 0);
}

#[test]
fn queued_general_destinations_survive_army_formation_and_lead_each_leg() {
    let build = || {
        let mut world = world();
        let soldier = world
            .spawn_kind(PlayerId(0), UnitKind::Militia, at(10, 11))
            .unwrap();
        let general = world
            .spawn_kind(PlayerId(0), UnitKind::General, at(10, 10))
            .unwrap();
        assert_eq!(world.general_controller(soldier), Some(general));
        world.issue_move(general, at(30, 10)).unwrap();
        world.issue_queued_move(general, at(30, 30)).unwrap();
        (world, general, soldier)
    };
    let (mut first, general, soldier) = build();
    let (mut second, _, _) = build();
    for _ in 0..20 {
        first.advance();
        second.advance();
        assert_eq!(first.canonical_hash(), second.canonical_hash());
    }
    assert_eq!(
        first.move_destinations(general).last(),
        Some(&at(30, 30)),
        "forming the General's army must retain the owner's later destination"
    );
    for world in [&mut first, &mut second] {
        world.issue_queued_move(general, at(10, 30)).unwrap();
        assert_eq!(
            world.move_destinations(general),
            [at(30, 10), at(30, 30), at(10, 30)],
            "a waypoint appended after forming the army must extend its existing route"
        );
    }
    let mut first_leg = false;
    let mut second_leg = false;
    for _ in 0..2000 {
        first.advance();
        second.advance();
        assert_eq!(first.canonical_hash(), second.canonical_hash());
        let p = first.unit(general).unwrap().position;
        first_leg |= (p.x - at(30, 10).x).abs() < 2048 && (p.y - at(30, 10).y).abs() < 2048;
        second_leg |=
            first_leg && (p.x - at(30, 30).x).abs() < 2048 && (p.y - at(30, 30).y).abs() < 2048;
    }
    assert!(
        first_leg,
        "the General must visit the first leg before turning"
    );
    assert!(
        second_leg,
        "the General must visit the second leg before turning"
    );
    // Infantry keeps its existing eight-tile forward role offset. The final
    // leg heads west, so the soldier's destination is west of his commander.
    for (id, goal) in [(general, at(10, 30)), (soldier, at(2, 30))] {
        let p = first.unit(id).unwrap().position;
        assert!(
            (p.x - goal.x).abs() < 2 * 1024 && (p.y - goal.y).abs() < 2 * 1024,
            "the whole army must follow the queued third leg: {id:?} at {p:?}"
        );
    }
}

#[test]
fn queued_general_retreat_keeps_the_unfinished_player_leg() {
    check_general_retreat_queue(18);
}

#[test]
fn queued_general_retreat_home_snapshot_keeps_the_unfinished_player_leg() {
    check_general_retreat_queue(22);
}

fn check_general_retreat_queue(enemy_x: i32) {
    let mut world = world();
    world.set_fog_disabled(true);
    world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(10, 11))
        .unwrap();
    let general = world
        .spawn_kind(PlayerId(0), UnitKind::General, at(10, 10))
        .unwrap();
    world.issue_move(general, at(30, 10)).unwrap();
    world.issue_queued_move(general, at(30, 30)).unwrap();
    for _ in 0..20 {
        world.advance();
    }
    // Wound the commander so a visible enemy forces a temporary retreat home.
    // Enemies never attack, so survival cannot mask queue behavior.
    let index = world.lookup[&general];
    world.units[index].state.hp = 1;
    for n in 0..8 {
        let enemy = world
            .spawn_kind(PlayerId(1), UnitKind::Knight, at(enemy_x, 10 + n))
            .unwrap();
        world
            .issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
            .unwrap();
    }
    let mut retreated = false;
    for _ in 0..120 {
        world.advance();
        retreated |=
            world.general_status(general).unwrap().state == crate::GeneralState::Retreating;
    }
    assert!(retreated);
    assert_eq!(
        world.economy.generals.armies[&general].destination,
        Some(at(30, 10))
    );
    assert_eq!(
        world.move_queues.get(&general).and_then(|q| q.front()),
        Some(&at(30, 30))
    );
    // A save at retreat home contains that temporary settled formation.
    // Its completion on the first resumed tick must not skip the march leg.
    let snapshot = world.snapshot([0; 32]).unwrap();
    let record = snapshot
        .units
        .iter()
        .find(|r| r.unit.id == general)
        .unwrap();
    assert_eq!(record.queue, [at(30, 30)]);
    let mut restored = super::world();
    restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    assert_eq!(
        restored.move_queues.get(&general).and_then(|q| q.front()),
        Some(&at(30, 30)),
        "resume must restore FIFO before ticking: move={:?}, attack={:?}",
        record.move_to,
        record.unit.attack_target
    );
    restored.advance();
    assert_eq!(
        restored.economy.generals.armies[&general].destination,
        Some(at(30, 10))
    );
    assert_eq!(
        restored.move_queues.get(&general).and_then(|q| q.front()),
        Some(&at(30, 30))
    );
}

#[test]
fn queued_general_only_moves_keep_the_army_holding_on_every_leg() {
    let mut world = world();
    let soldier = world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(10, 11))
        .unwrap();
    let general = world
        .spawn_kind(PlayerId(0), UnitKind::General, at(10, 10))
        .unwrap();
    let held = world.unit(soldier).unwrap().position;
    world
        .issue_general_move(PlayerId(0), general, at(30, 10))
        .unwrap();
    world.issue_queued_move(general, at(30, 30)).unwrap();
    for _ in 0..1000 {
        world.advance();
    }
    assert_eq!(world.unit(general).unwrap().position, at(30, 30));
    assert!(world.economy.generals.armies[&general].solo);
    assert_eq!(
        world.unit(soldier).unwrap().position,
        held,
        "queued General-only legs must never start an army march"
    );
}

#[test]
fn a_queued_owner_order_survives_detaching_one_soldier_from_a_general_formation() {
    let mut world = world();
    let soldier = world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(10, 11))
        .unwrap();
    world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(11, 11))
        .unwrap();
    let general = world
        .spawn_kind(PlayerId(0), UnitKind::General, at(10, 10))
        .unwrap();
    world.issue_move(general, at(30, 10)).unwrap();
    for _ in 0..20 {
        world.advance();
    }
    assert!(world.formation(soldier).is_some());
    world.issue_queued_move(soldier, at(30, 30)).unwrap();
    world.general_player_order(PlayerId(0), &[soldier], None);
    // Finish both the existing formation leg and the owner's next leg at
    // infantry speed (their combined route is longer than 1,000 ticks).
    for _ in 0..2000 {
        world.advance();
    }
    assert_eq!(world.general_controller(soldier), None);
    assert_eq!(
        world.unit(soldier).unwrap().position,
        at(30, 30),
        "re-forming the remaining army must not cancel the owner's queued order"
    );
}

#[test]
fn a_saved_detached_soldier_keeps_his_queued_owner_destination() {
    let mut world = world();
    let soldier = world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(10, 11))
        .unwrap();
    world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(11, 11))
        .unwrap();
    let general = world
        .spawn_kind(PlayerId(0), UnitKind::General, at(10, 10))
        .unwrap();
    world.issue_move(general, at(30, 10)).unwrap();
    for _ in 0..20 {
        world.advance();
    }
    world.issue_queued_move(soldier, at(30, 30)).unwrap();
    world.general_player_order(PlayerId(0), &[soldier], None);
    assert!(world.formation(soldier).is_some());
    let snapshot = world.snapshot([0; 32]).unwrap();
    let mut restored = super::world();
    restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    assert_eq!(restored.general_controller(soldier), None);
    assert_eq!(
        restored.move_destinations(soldier).last(),
        Some(&at(30, 30))
    );
    for _ in 0..2000 {
        restored.advance();
    }
    assert_eq!(restored.unit(soldier).unwrap().position, at(30, 30));
}

#[test]
fn mixed_general_formation_and_scalar_queues_share_one_atomic_destination_limit() {
    for formation_first in [false, true] {
        let mut world = world();
        let general = world
            .spawn_kind(PlayerId(0), UnitKind::General, at(10, 10))
            .unwrap();
        if formation_first {
            for n in 0..aoe_core::MAX_MOVE_DESTINATIONS {
                world
                    .issue_formation_move(
                        PlayerId(0),
                        &[general],
                        aoe_core::Formation::Line,
                        at(30 + n as i32, 10),
                        n > 0,
                        false,
                    )
                    .unwrap();
            }
        } else {
            world.issue_move(general, at(30, 10)).unwrap();
            for n in 1..aoe_core::MAX_MOVE_DESTINATIONS {
                world
                    .issue_queued_move(general, at(30 + n as i32, 10))
                    .unwrap();
            }
            // The General converts the current leg to his own singleton
            // formation while retaining all fifteen later player legs.
            for _ in 0..10 {
                world.advance();
            }
            assert!(world.formation(general).is_some());
        }
        assert_eq!(
            world.move_destinations(general).len(),
            aoe_core::MAX_MOVE_DESTINATIONS
        );
        let before = world.canonical_hash();
        let result = if formation_first {
            world.issue_queued_move(general, at(60, 10)).map(|_| ())
        } else {
            world.issue_formation_move(
                PlayerId(0),
                &[general],
                aoe_core::Formation::Line,
                at(60, 10),
                true,
                false,
            )
        };
        assert!(
            result.is_err(),
            "both append paths must count every pending destination"
        );
        assert_eq!(
            world.canonical_hash(),
            before,
            "an overfull queue is refused atomically"
        );
        assert_eq!(
            world.move_destinations(general).len(),
            aoe_core::MAX_MOVE_DESTINATIONS
        );
    }
}

#[test]
fn queued_moves_are_bounded_refused_atomically_and_cancelled_by_replacement_or_stop() {
    let mut world = world();
    let id = world.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    world.issue_queued_move(id, at(14, 10)).unwrap();
    for n in 1..aoe_core::MAX_MOVE_DESTINATIONS {
        world.issue_queued_move(id, at(14 + n as i32, 10)).unwrap();
    }
    let before = world.canonical_hash();
    assert!(world.issue_queued_move(id, at(50, 10)).is_err());
    assert_eq!(world.canonical_hash(), before);
    assert!(
        world
            .issue_queued_move(id, WorldPosition::new(-10, 0))
            .is_err()
    );
    assert_eq!(world.canonical_hash(), before);
    world.issue_move(id, at(10, 14)).unwrap();
    assert!(world.move_destinations(id).is_empty());
    world.issue_queued_move(id, at(20, 14)).unwrap();
    world.issue_stop(PlayerId(0), id).unwrap();
    assert!(world.move_destinations(id).is_empty());
    world.advance();
    assert!(!world.unit(id).unwrap().moving);
}

#[test]
fn pending_destinations_participate_in_the_canonical_hash() {
    let mut first = world();
    let mut second = world();
    let a = first.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    let b = second.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    first.issue_move(a, at(14, 10)).unwrap();
    second.issue_move(b, at(14, 10)).unwrap();
    first.issue_queued_move(a, at(14, 14)).unwrap();
    second.issue_queued_move(b, at(14, 15)).unwrap();
    assert_eq!(first.unit(a), second.unit(b));
    assert_ne!(first.canonical_hash(), second.canonical_hash());
}

#[test]
fn fixed_speed_has_equal_distance_for_cardinal_and_diagonal_orders() {
    let mut cardinal = world();
    let mut diagonal = world();
    let start = WorldPosition::new(10 * FIXED_SUBUNITS_PER_TILE, 10 * FIXED_SUBUNITS_PER_TILE);
    let a = cardinal.spawn_unit(PlayerId(0), start).unwrap();
    let b = diagonal.spawn_unit(PlayerId(0), start).unwrap();
    cardinal
        .issue_move(a, WorldPosition::new(start.x + 5_000, start.y))
        .unwrap();
    diagonal
        .issue_move(b, WorldPosition::new(start.x + 5_000, start.y + 5_000))
        .unwrap();
    assert_eq!(cardinal.movement_order(a).unwrap().travelled, 0);
    for _ in 0..8 {
        cardinal.advance();
        diagonal.advance();
    }
    assert_eq!(cardinal.movement_order(a).unwrap().travelled, 1_024);
    assert_eq!(diagonal.movement_order(b).unwrap().travelled, 1_024);
}

#[test]
fn exact_arrival_and_mid_move_redirection_are_deterministic() {
    let mut first = world();
    let mut second = world();
    let start = WorldPosition::new(20_000, 20_000);
    let a = first.spawn_unit(PlayerId(0), start).unwrap();
    let b = second.spawn_unit(PlayerId(0), start).unwrap();
    let target = WorldPosition::new(23_000, 24_000);
    first.issue_move(a, target).unwrap();
    second.issue_move(b, target).unwrap();
    for _ in 0..100 {
        first.advance();
        second.advance();
    }
    assert_eq!(
        first.unit(a),
        second.unit(b).map(|unit| GameUnit { id: a, ..unit })
    );
    assert_eq!(
        first.unit(a).unwrap().position,
        WorldConfig::default().snap_ground_position(target)
    );
    assert!(!first.unit(a).unwrap().moving);
    first
        .issue_move(a, WorldPosition::new(25_000, 24_000))
        .unwrap();
    first.advance();
    assert!(first.unit(a).unwrap().moving);
}
