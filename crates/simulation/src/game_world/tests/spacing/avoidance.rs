//! Exercise movement alone: idle separation must never repair these arrivals.
use super::*;

#[test]
fn a_short_move_to_an_occupied_tile_does_not_cancel_at_its_origin() {
    let mut w = world();
    let origin = at(30, 30);
    let click = at(31, 30);
    let scout = w.spawn_unit(PlayerId(0), origin).unwrap();
    let occupant = w.spawn_villager(PlayerId(0), click).unwrap();
    w.issue_move(scout, click).unwrap();
    // Tick zero is an arrival-retry tick for this scout. The nearest free
    // tile search must not turn the new order into a move to its own origin.
    w.advance_movement();
    assert_ne!(w.unit(scout).unwrap().position, origin);
    for tick in 0..100 {
        w.advance_movement();
        clearance(&w, &[scout, occupant], tick);
    }
    let arrival = w.unit(scout).unwrap().position;
    assert_ne!(arrival.tile_floor(), origin.tile_floor());
    assert!(distance(arrival, click) <= 2 * 1024_i64.pow(2));
    assert!(distance(arrival, w.unit(occupant).unwrap().position) >= 640_i64.pow(2));
    assert!(w.movement_order(scout).is_none());
    assert_eq!(w.movement_failure(scout), None);
    assert_eq!(w.unit(occupant).unwrap().position, click);
}

fn clearance(w: &GameWorld, ids: &[EntityId], tick: usize) {
    for (n, &a) in ids.iter().enumerate() {
        for &b in &ids[n + 1..] {
            assert!(
                distance(w.unit(a).unwrap().position, w.unit(b).unwrap().position)
                    >= 640_i64.pow(2),
                "overlap {a:?}/{b:?} at tick {tick}: {:?}/{:?}",
                w.unit(a).unwrap().position,
                w.unit(b).unwrap().position,
            );
        }
    }
}

#[test]
fn local_avoidance_passes_a_line_of_idle_units_without_merging_or_pushing() {
    let mut w = world();
    let idle = (27..=33)
        .map(|y| w.spawn_villager(PlayerId(0), at(40, y)).unwrap())
        .collect::<Vec<_>>();
    let mover = w.spawn_villager(PlayerId(0), at(30, 30)).unwrap();
    let mut ids = idle.clone();
    ids.push(mover);
    w.issue_move(mover, at(50, 30)).unwrap();
    for tick in 0..1200 {
        w.advance_movement();
        clearance(&w, &ids, tick);
    }
    assert_eq!(w.unit(mover).unwrap().position, at(50, 30));
    for (&id, y) in idle.iter().zip(27..=33) {
        assert_eq!(w.unit(id).unwrap().position, at(40, y));
    }
}

#[test]
fn local_avoidance_two_groups_cross_without_merging_and_finish_in_bounded_time() {
    for perpendicular in [false, true] {
        let mut w = world();
        let mut ids = Vec::new();
        for n in 0..6 {
            let a = w.spawn_villager(PlayerId(0), at(30, 28 + n)).unwrap();
            let (start, end) = if perpendicular {
                (at(38 + n, 20), at(38 + n, 45))
            } else {
                (at(50, 28 + n), at(30, 28 + n))
            };
            let b = w.spawn_villager(PlayerId(0), start).unwrap();
            w.issue_move(a, at(55, 28 + n)).unwrap();
            w.issue_move(b, end).unwrap();
            ids.extend([a, b]);
        }
        for tick in 0..1500 {
            w.advance_movement();
            clearance(&w, &ids, tick);
        }
        assert!(ids.iter().all(|&id| w.movement_order(id).is_none()));
    }
}

#[test]
fn local_avoidance_arrivals_keep_distinct_tiles_and_replace_an_occupied_slot() {
    for occupied in [false, true] {
        let mut w = world();
        let ids = group(&mut w, false);
        for &id in &ids {
            w.issue_move(id, at(50, 30)).unwrap();
        }
        // Optionally a worker reaches the click after slots were assigned.
        let worker = occupied.then(|| w.spawn_villager(PlayerId(0), at(50, 30)).unwrap());
        let mut all = ids.clone();
        all.extend(worker);
        for tick in 0..1400 {
            w.advance_movement();
            clearance(&w, &all, tick);
        }
        assert!(
            ids.iter().all(|&id| w.movement_order(id).is_none()),
            "{:?}",
            ids.iter()
                .map(|&id| (
                    id,
                    w.unit(id).unwrap().position,
                    w.movement_order(id),
                    w.queued_waypoint_count(id)
                ))
                .collect::<Vec<_>>()
        );
        let tiles = all
            .iter()
            .map(|&id| w.unit(id).unwrap().position.tile_floor())
            .collect::<BTreeSet<_>>();
        assert_eq!(tiles.len(), all.len());
        if let Some(worker) = worker {
            assert_eq!(w.unit(worker).unwrap().position, at(50, 30));
        }
        assert!(
            tiles
                .iter()
                .all(|t| t.x.abs_diff(50) <= 3 && t.y.abs_diff(30) <= 3)
        );
    }
}

#[test]
fn local_avoidance_slots_skip_idle_occupants_in_nearest_tile_order() {
    let mut w = world();
    let worker = w.spawn_villager(PlayerId(0), at(50, 30)).unwrap();
    let mover = w.spawn_villager(PlayerId(0), at(40, 30)).unwrap();
    w.issue_move(mover, at(50, 30)).unwrap();
    // Reserve the requested slot first: its occupant can depart before arrival
    // (including two units swapping places). Recheck occupancy on approach.
    assert_eq!(w.movement_order(mover).unwrap().destination, at(50, 30));
    for _ in 0..600 {
        w.advance_movement();
    }
    assert_eq!(w.unit(mover).unwrap().position, at(49, 30));
    assert_eq!(w.unit(worker).unwrap().position, at(50, 30));

    // With the first ring full and the western radius-two slot occupied, a
    // sqrt(5)-distant tile must not win over the free radius-two northern slot.
    let mut w = world();
    for x in 49..=51 {
        for y in 29..=31 {
            w.spawn_villager(PlayerId(0), at(x, y)).unwrap();
        }
    }
    w.spawn_villager(PlayerId(0), at(48, 30)).unwrap();
    let mover = w.spawn_villager(PlayerId(0), at(40, 30)).unwrap();
    w.issue_move(mover, at(50, 30)).unwrap();
    for _ in 0..800 {
        w.advance_movement();
    }
    assert_eq!(w.unit(mover).unwrap().position, at(50, 28));
}

#[test]
fn local_avoidance_passes_a_working_lumberjack_without_interrupting_work() {
    let mut w = super::super::far_orders::wide_world();
    let tree = aoe_map::ResourceNode::starting(TileCoord::new(40, 31), aoe_map::ResourceKind::Wood)
        .unwrap();
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        overlay.insert_starting_resource(generator, tree).unwrap();
    }
    let worker = w.spawn_villager(PlayerId(0), at(40, 30)).unwrap();
    w.issue_gather(PlayerId(0), worker, tree.id).unwrap();
    run(&mut w, 3);
    assert_eq!(w.unit(worker).unwrap().task, UnitTask::Gathering);
    let stand = w.unit(worker).unwrap().position;
    let mover = w.spawn_villager(PlayerId(0), at(35, 30)).unwrap();
    w.issue_move(mover, at(45, 30)).unwrap();
    for tick in 0..400 {
        w.advance();
        clearance(&w, &[worker, mover], tick);
        assert_eq!(w.unit(worker).unwrap().position, stand);
    }
    assert_eq!(w.unit(mover).unwrap().position, at(45, 30));
    assert_eq!(w.gather_target(worker), Some(tree.id));
    assert!(w.unit(worker).unwrap().carried.unwrap().amount > 0);
}
