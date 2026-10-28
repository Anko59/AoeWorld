use super::*;
use aoe_core::Formation;

const FORMS: [Formation; 4] = [
    Formation::Line,
    Formation::Box,
    Formation::Staggered,
    Formation::Flank,
];

fn run(w: &mut GameWorld, ticks: usize) {
    for _ in 0..ticks {
        w.advance();
    }
}

#[test]
fn formations_layouts_are_centered_distinct_and_oriented() {
    let mut layouts = BTreeSet::new();
    for form in FORMS {
        let slots = crate::game_world::formation::layout::slots(form, 20);
        assert_eq!(slots.len(), 20);
        let unique: BTreeSet<_> = slots.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(unique.len(), 20);
        assert!(slots.iter().map(|p| i64::from(p.x)).sum::<i64>().abs() < 20);
        assert!(slots.iter().map(|p| i64::from(p.y)).sum::<i64>().abs() < 20);
        assert!(layouts.insert(unique.into_iter().collect::<Vec<_>>()));
        let mut w = wide_world();
        let ids = army(&mut w, 20);
        w.issue_formation_move(PlayerId(0), &ids, form, center(70, 50), false, false)
            .unwrap();
        run(&mut w, 1800);
        for &id in &ids {
            let u = w.unit(id).unwrap();
            assert_eq!(u.position, w.formation_slot(id).unwrap(), "{form:?} {id:?}");
            assert!(!u.moving);
            assert_eq!(u.facing, w.unit(ids[0]).unwrap().facing);
        }
    }
}

#[test]
fn formations_share_one_centroid_route_and_use_the_slowest_speed() {
    let mut w = wide_world();
    let ids = vec![
        w.spawn_kind(PlayerId(0), UnitKind::Scout, center(40, 40))
            .unwrap(),
        w.spawn_kind(PlayerId(0), UnitKind::Archer, center(40, 42))
            .unwrap(),
    ];
    let from = center(40, 41);
    let before = w.route_scheduler_stats().metrics.orders;
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(180, 41),
        false,
        false,
    )
    .unwrap();
    assert_eq!(w.formation_anchor(ids[0]), Some(from));
    assert_eq!(w.route_scheduler_stats().metrics.orders - before, 1);
    run(&mut w, 100);
    let start = w.formation_anchor(ids[0]).unwrap();
    let (speed, denom) = w.movement_speed(UnitKind::Archer, PlayerId(0));
    let carry = w.units[w.lookup[&ids[0]]].order.unwrap().speed_carry;
    run(&mut w, 100);
    let end = w.formation_anchor(ids[0]).unwrap();
    // Speed matching preserves fractional credit earned while forming.
    assert_eq!(end.x - start.x, 4916);
    assert_eq!(
        end.x - start.x,
        ((100 * speed * 4 + carry) / (denom * 4)) as i32
    );
    assert_eq!(end.y, start.y);
    assert_eq!(w.route_scheduler_stats().metrics.orders - before, 1);
    assert!(
        ids.iter()
            .skip(1)
            .all(|id| w.units[w.lookup[id]].planner.is_none())
    );
    assert_eq!(w.route_accounting_error(), None);
}

#[test]
fn formations_squeeze_through_a_one_tile_gap_then_reform() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 20);
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        for y in 0..100 {
            if y != 50 {
                overlay
                    .insert_starting_resource(
                        generator,
                        aoe_map::ResourceNode::starting(
                            TileCoord::new(65, y),
                            aoe_map::ResourceKind::Wood,
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
        }
    }
    w.prepare_navigation(&|| false).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(90, 50),
        false,
        false,
    )
    .unwrap();
    for _ in 0..3000 {
        w.advance();
        for &id in &ids {
            let tile = w.unit(id).unwrap().position.tile_floor();
            assert!(tile.x != 65 || tile.y == 50, "crossed wall: {tile:?}");
        }
    }
    for &id in &ids {
        assert_eq!(
            w.unit(id).unwrap().position,
            w.formation_slot(id).unwrap(),
            "{id:?} anchor={:?}",
            w.formation_anchor(ids[0])
        );
        assert!(w.unit(id).unwrap().position.x > 65 * 1024);
    }
    assert_eq!(w.route_scheduler_stats().metrics.orders, 1);
}

#[test]
fn formations_queue_turns_and_replay_in_canonical_member_order() {
    let mut a = wide_world();
    let mut b = wide_world();
    let ids = army(&mut a, 10);
    let mut reversed = army(&mut b, 10);
    reversed.reverse();
    for (w, group) in [(&mut a, &ids), (&mut b, &reversed)] {
        w.issue_formation_move(
            PlayerId(0),
            group,
            Formation::Box,
            center(65, 45),
            false,
            false,
        )
        .unwrap();
        w.issue_formation_move(
            PlayerId(0),
            group,
            Formation::Box,
            center(65, 65),
            true,
            false,
        )
        .unwrap();
        assert_eq!(
            w.move_destinations(ids[1]),
            vec![center(65, 45), center(65, 65)]
        );
    }
    for _ in 0..2400 {
        a.advance();
        b.advance();
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }
    assert_eq!(a.formation_anchor(ids[0]), Some(center(65, 65)));
    assert!(a.move_destinations(ids[1]).is_empty());
    assert_eq!(a.route_scheduler_stats().metrics.orders, 2);
}

#[test]
fn formations_reject_invalid_groups_atomically_and_release_only_the_attacker() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 4);
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(50, 40))
        .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Flank,
        center(80, 40),
        false,
        false,
    )
    .unwrap();
    for invalid in [vec![], vec![ids[0], ids[0]], vec![ids[0], enemy]] {
        let before = w.canonical_hash();
        assert!(
            w.issue_formation_move(
                PlayerId(0),
                &invalid,
                Formation::Box,
                center(80, 40),
                false,
                false
            )
            .is_err()
        );
        assert_eq!(before, w.canonical_hash());
    }
    w.issue_attack(PlayerId(0), ids[1], enemy).unwrap();
    // Only the attacker leaves; the other three keep marching in their group.
    assert!(w.formation(ids[1]).is_none());
    assert!(
        [ids[0], ids[2], ids[3]]
            .iter()
            .all(|&id| w.formation(id) == Some(Formation::Flank))
    );
    assert_eq!(w.unit(ids[1]).unwrap().attack_target, Some(enemy));
    assert_eq!(w.route_accounting_error(), None);
}

#[test]
fn formations_attack_move_engages_visible_enemies_member_by_member_and_stops_release_the_group_unit_by_unit()
 {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 4);
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(55, 40))
        .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(80, 40),
        false,
        true,
    )
    .unwrap();
    let mut first_hit = None;
    for tick in 1..=500 {
        w.advance();
        if first_hit.is_none() && w.unit(enemy).is_none_or(|u| u.hp < u.max_hp) {
            let left = ids.iter().filter(|&&id| w.formation(id).is_none()).count();
            first_hit = Some((tick, left));
        }
    }
    // Contact is per member: all four came to see the lone enemy and left the
    // group to fight it (the first blow at tick 262, 315 before PR 6's slots: members look within their
    // own 4-tile sight, not a 13x13-tile window, which struck at tick 275). Once it died each one marched on again, as
    // a group of one with the same attack-move order and its own slot.
    assert_eq!(first_hit, Some((262, 4)));
    assert!(w.unit(enemy).is_none_or(|u| u.hp == 0));
    assert!(
        ids.iter()
            .all(|&id| { w.formation(id) == Some(Formation::Line) && w.unit(id).unwrap().moving })
    );
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Box,
        center(80, 40),
        false,
        false,
    )
    .unwrap();
    // A stop releases only that unit; the rest of the group keeps marching.
    w.issue_stop(PlayerId(0), ids[0]).unwrap();
    assert!(w.formation(ids[0]).is_none());
    assert!(
        ids[1..]
            .iter()
            .all(|&id| w.formation(id) == Some(Formation::Box))
    );
    for &id in &ids[1..] {
        w.issue_stop(PlayerId(0), id).unwrap();
    }
    assert!(ids.iter().all(|&id| w.formation(id).is_none()));
    assert_eq!(w.route_scheduler_stats().planners, 0);
}

#[test]
fn large_bodies_finish_a_diagonal_formation_without_rounding_into_each_other() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = [
        w.spawn_kind(PlayerId(0), UnitKind::Mangonel, center(40, 40))
            .unwrap(),
        w.spawn_kind(PlayerId(0), UnitKind::Mangonel, center(40, 42))
            .unwrap(),
    ];
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(52, 52),
        false,
        false,
    )
    .unwrap();
    let mut previous = ids.map(|id| w.unit(id).unwrap().position);
    let mut stalled = [0; 2];
    for tick in 0..1600 {
        w.advance();
        for (n, &id) in ids.iter().enumerate() {
            let at = w.unit(id).unwrap().position;
            if at != previous[n] || Some(at) == w.formation_slot(id) {
                stalled[n] = 0;
            } else {
                stalled[n] += 1;
            }
            assert!(
                stalled[n] <= 240,
                "siege member {id:?} stalled at tick {tick}"
            );
            previous[n] = at;
        }
        let a = w.unit(ids[0]).unwrap().position;
        let b = w.unit(ids[1]).unwrap().position;
        assert!(i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2) >= 1024_i64.pow(2));
    }
    for id in ids {
        assert_eq!(w.unit(id).unwrap().position, w.formation_slot(id).unwrap());
        assert!(!w.unit(id).unwrap().moving);
    }
}
