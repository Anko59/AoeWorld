//! A shared anchor must feed a spaced column through the pass, then re-form.
use super::*;
use aoe_core::Formation;

fn forest_pass_army() -> (GameWorld, Vec<EntityId>) {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let origin = center(40, 50);
    let ids = crate::game_world::formation::layout::slots(Formation::Line, 20)
        .into_iter()
        .map(|offset| {
            let p = WorldPosition::new(origin.x + offset.y, origin.y + offset.x);
            w.spawn_kind(PlayerId(0), UnitKind::Militia, p).unwrap()
        })
        .collect::<Vec<_>>();
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        for x in 64..70 {
            for y in 0..100 {
                if !(49..=50).contains(&y) {
                    overlay
                        .insert_starting_resource(
                            generator,
                            aoe_map::ResourceNode::starting(
                                TileCoord::new(x, y),
                                aoe_map::ResourceKind::Wood,
                            )
                            .unwrap(),
                        )
                        .unwrap();
                }
            }
        }
    }
    w.prepare_navigation(&|| false).unwrap();
    (w, ids)
}

#[test]
fn twenty_unit_formation_crosses_a_two_tile_forest_pass_without_stacking() {
    let (mut w, ids) = forest_pass_army();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(95, 50),
        false,
        false,
    )
    .unwrap();
    let mut crossed = BTreeSet::new();
    for tick in 0..3500 {
        // No idle separation: the formation's movement must maintain clearance.
        w.advance_movement();
        w.refill_route_budgets();
        for (n, &id) in ids.iter().enumerate() {
            let position = w.unit(id).unwrap().position;
            let tile = position.tile_floor();
            if (64..70).contains(&tile.x) {
                assert!((49..=50).contains(&tile.y), "tree crossed: {tile:?}");
                crossed.insert(id);
            }
            for &other in &ids[n + 1..] {
                let other = w.unit(other).unwrap().position;
                let dx = i64::from(position.x) - i64::from(other.x);
                let dy = i64::from(position.y) - i64::from(other.y);
                assert!(
                    dx * dx + dy * dy >= 410 * 410,
                    "stack at tick {tick}: {id:?}"
                );
            }
        }
    }
    assert_eq!(
        crossed.len(),
        20,
        "every member must traverse the pass; anchor={:?} members={:?}",
        w.formation_anchor(ids[0]),
        ids.iter()
            .map(|&id| (id, w.unit(id).unwrap().position, w.formation_slot(id)))
            .collect::<Vec<_>>()
    );
    assert_eq!(w.formation_anchor(ids[0]), Some(center(95, 50)));
    for &id in &ids {
        let unit = w.unit(id).unwrap();
        assert_eq!(
            unit.position,
            w.formation_slot(id).unwrap(),
            "stuck: {id:?}"
        );
        assert!(!unit.moving, "still moving: {id:?}");
    }
    assert_eq!(w.route_scheduler_stats().metrics.orders, 1);
}

#[test]
fn queued_waypoint_inside_forest_pass_continues_on_anchor_arrival() {
    let (mut w, ids) = forest_pass_army();
    let (mut replay, mut reversed) = forest_pass_army();
    reversed.reverse();
    let waypoint = center(67, 50);
    let destination = center(95, 50);
    for (world, members) in [(&mut w, &ids), (&mut replay, &reversed)] {
        world
            .issue_formation_move(
                PlayerId(0),
                members,
                Formation::Line,
                waypoint,
                false,
                false,
            )
            .unwrap();
        world
            .issue_formation_move(
                PlayerId(0),
                members,
                Formation::Line,
                destination,
                true,
                false,
            )
            .unwrap();
    }
    let mut reached_waypoint = false;
    for _ in 0..3500 {
        w.advance();
        replay.advance();
        assert_eq!(w.canonical_hash(), replay.canonical_hash());
        if w.formation_anchor(ids[0]) == Some(waypoint) && !reached_waypoint {
            reached_waypoint = true;
            assert_eq!(
                w.move_destinations(ids[0]),
                vec![destination],
                "anchor arrival must consume the waypoint before wide slots can re-form"
            );
        }
    }
    assert!(
        reached_waypoint,
        "anchor must reach the waypoint in the pass"
    );
    assert_eq!(w.formation_anchor(ids[0]), Some(destination));
    assert!(w.move_destinations(ids[0]).is_empty());
    for &id in &ids {
        let unit = w.unit(id).unwrap();
        assert_eq!(unit.position, w.formation_slot(id).unwrap(), "{id:?}");
        assert!(!unit.moving, "still moving: {id:?}");
    }
    assert_eq!(w.route_scheduler_stats().metrics.orders, 2);
    assert_eq!(w.route_accounting_error(), None);
}

#[test]
fn final_destination_inside_forest_pass_settles_in_column() {
    let (mut w, ids) = forest_pass_army();
    let destination = center(67, 50);
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        destination,
        false,
        false,
    )
    .unwrap();
    for _ in 0..3500 {
        w.advance();
    }
    assert_eq!(w.formation_anchor(ids[0]), Some(destination));
    assert!(
        w.move_destinations(ids[0]).is_empty(),
        "a final destination in the pass must finish without waiting for wide slots"
    );
    let positions = ids
        .iter()
        .map(|&id| {
            let unit = w.unit(id).unwrap();
            assert!(!unit.moving, "still moving: {id:?}");
            assert_eq!(unit.task, UnitTask::Idle);
            unit.position
        })
        .collect::<Vec<_>>();
    assert!(
        ids.iter()
            .any(|&id| { w.unit(id).unwrap().position != w.formation_slot(id).unwrap() })
    );
    let work = w.route_scheduler_stats().metrics.formation_steering_work;
    for _ in 0..128 {
        w.advance();
        for (&id, &position) in ids.iter().zip(&positions) {
            assert_eq!(w.unit(id).unwrap().position, position, "{id:?}");
            assert!(!w.unit(id).unwrap().moving);
        }
    }
    assert_eq!(
        w.route_scheduler_stats().metrics.formation_steering_work,
        work
    );
    assert_eq!(w.route_scheduler_stats().metrics.orders, 1);
    assert_eq!(w.route_accounting_error(), None);
}

#[test]
fn every_member_makes_progress_through_the_two_tile_gap() {
    let (mut w, ids) = forest_pass_army();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(95, 50),
        false,
        false,
    )
    .unwrap();
    let mut previous = ids
        .iter()
        .map(|&id| (w.unit(id).unwrap().position, 0))
        .collect::<Vec<_>>();
    for tick in 0..3500 {
        w.advance();
        for (n, &id) in ids.iter().enumerate() {
            let u = w.unit(id).unwrap();
            if u.position != previous[n].0 || (!u.moving && u.position.tile_floor().x > 85) {
                previous[n].1 = 0;
            } else {
                previous[n].1 += 1;
            }
            assert!(
                previous[n].1 <= 240,
                "member {id:?} stalled at tick {tick}: {u:?}, slot={:?}, formations={:?}, units={:?}",
                w.formation_slot(id),
                w.formations,
                ids.iter().map(|id| w.unit(*id)).collect::<Vec<_>>()
            );
            for &other in &ids[n + 1..] {
                let p = w.unit(other).unwrap().position;
                let dx = i64::from(u.position.x) - i64::from(p.x);
                let dy = i64::from(u.position.y) - i64::from(p.y);
                assert!(
                    dx * dx + dy * dy >= 410 * 410,
                    "radius collision at {tick}: {id:?}/{other:?}"
                );
            }
            previous[n].0 = u.position;
        }
    }
    for id in ids {
        assert_eq!(
            w.unit(id).unwrap().position,
            w.formation_slot(id).unwrap(),
            "{id:?}"
        );
    }
}
