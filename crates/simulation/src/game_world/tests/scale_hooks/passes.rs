//! The whole-population pass counters: diagnostics that count and change nothing.
use super::*;

fn count(world: &GameWorld, name: &str) -> crate::PassCount {
    world
        .pass_counts()
        .into_iter()
        .find_map(|(pass, count)| (pass == name).then_some(count))
        .expect("a named pass")
}

#[test]
fn every_named_pass_is_reported_from_zero() {
    let w = world();
    let names: Vec<_> = w.pass_counts().into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, crate::PASS_NAMES);
    assert!(
        w.pass_counts()
            .into_iter()
            .all(|(_, count)| count == crate::PassCount::default())
    );
}

#[test]
fn the_tick_passes_count_each_tick_and_the_hash_ignores_them() {
    let mut counted = busy_world();
    let mut unread = busy_world();
    for _ in 0..30 {
        counted.advance();
        let _ = counted.pass_counts();
        unread.advance();
    }
    assert_eq!(counted.canonical_hash(), unread.canonical_hash());
    // `collect_changed` closes every tick, whatever else the tick skipped.
    assert_eq!(count(&counted, "collect_changed").calls, 30);
    // Dirty bits replaced its sort: the forbidden pass visits nothing.
    assert_eq!(count(&counted, "collect_changed").visited, 0);
    assert!(count(&counted, "tables_refresh").calls > 0);
    assert_eq!(count(&counted, "advance_transports").visited, 0);
    // Replaced by events and indexes: these whole-population passes never run.
    for replaced in ["advance_gates", "remember_visible_buildings"] {
        assert_eq!(count(&counted, replaced), crate::PassCount::default());
    }
}

#[test]
fn collision_closed_accesses_only_subscribed_units() {
    collision_accesses_only_subscribed_units(false);
}

#[test]
fn collision_opened_accesses_only_subscribed_units() {
    collision_accesses_only_subscribed_units(true);
}

fn collision_accesses_only_subscribed_units(opened: bool) {
    let mut w = busy_world();
    // Idle bystanders a full scan would visit.
    for n in 0..100 {
        w.spawn_unit(PlayerId(0), at(150 + n % 10 * 2, 150 + n / 10 * 2))
            .unwrap();
    }
    let far = w.spawn_unit(PlayerId(0), at(200, 200)).unwrap();
    let index = w.lookup[&far];
    w.set_movement_error(index, Some(crate::GameWorldError::Unreachable));
    w.remember_unreachable(index, TileCoord::new(5, 5));
    let area = TileRect::new(TileCoord::new(10, 10), TileCoord::new(12, 12));
    w.units.reset_accesses();
    if opened {
        w.collision_opened(area);
    } else {
        w.collision_closed(area);
    }
    let accesses = w.units.accesses();
    // A closing clears every error, wherever the unit stands, but no memo.
    assert_eq!(w.units[index].last_movement_error, None);
    assert_eq!(w.units[index].unreachable_memo.is_none(), opened);
    let pass = if opened {
        "collision_opened"
    } else {
        "collision_closed"
    };
    let counted = count(&w, pass);
    assert_eq!(counted.calls, 1, "{pass}");
    assert!(counted.visited <= 4, "{pass} visited {}", counted.visited);
    // Measured at the store boundary, independently of pass/subscriber counts.
    // A bulk view charges all exposed slots; indexing charges one per access.
    // No timing threshold: even a read-only full scan must exceed this bound.
    assert!(w.units.len() > 100);
    assert!(accesses <= 16, "opened={opened}: {accesses} unit accesses");
}

#[test]
fn unit_access_probe_covers_indexing_iteration_and_slice_exposure() {
    let mut w = busy_world();
    let len = w.units.len();
    w.units.reset_accesses();
    std::hint::black_box(&w.units[0]);
    std::hint::black_box(&mut w.units[1]);
    assert_eq!(w.units.accesses(), 2);
    w.units.reset_accesses();
    std::hint::black_box(&w.units[..1]);
    std::hint::black_box(&mut w.units[1..]);
    assert_eq!(w.units.accesses(), len);
    w.units.reset_accesses();
    for unit in &w.units {
        std::hint::black_box(unit);
    }
    assert_eq!(w.units.accesses(), len);
    w.units.reset_accesses();
    for unit in &mut w.units {
        std::hint::black_box(unit);
    }
    assert_eq!(w.units.accesses(), len);
    w.units.reset_accesses();
    std::hint::black_box(w.units.as_slice());
    assert_eq!(w.units.accesses(), len);
    w.units.reset_accesses();
    std::hint::black_box(w.units.as_mut_slice());
    assert_eq!(w.units.accesses(), len);
    w.units.reset_accesses();
    for index in 0..w.units.len() {
        std::hint::black_box(&w.units[index]);
    }
    assert_eq!(w.units.accesses(), len);
}

#[test]
fn cleared_errors_unsubscribe_before_the_next_collision() {
    let mut w = world();
    for action in 0..4 {
        let id = w.spawn_unit(PlayerId(0), at(20 + action * 3, 20)).unwrap();
        let index = w.lookup[&id];
        w.set_movement_error(index, Some(crate::GameWorldError::Unreachable));
        assert!(w.collision_subscribers.errors.contains(&id));
        match action {
            0 => w.set_movement_error(index, None),
            1 => {
                w.issue_move(id, at(40, 40)).unwrap();
            }
            2 => {
                w.issue_move(id, w.units[index].state.position).unwrap();
            }
            _ => w.issue_stop(PlayerId(0), id).unwrap(),
        }
        assert!(w.units[index].last_movement_error.is_none());
        assert!(
            !w.collision_subscribers.errors.contains(&id),
            "action {action}"
        );
    }
}

#[test]
fn removing_units_unsubscribes_errors_memos_and_planners_without_collision() {
    let mut w = world();
    for _ in 0..128 {
        let id = w.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
        let index = w.lookup[&id];
        w.set_movement_error(index, Some(crate::GameWorldError::Unreachable));
        w.remember_unreachable(index, TileCoord::new(40, 40));
        w.store_planner(
            index,
            Some(aoe_map::RoutePlanner::new(
                TileCoord::new(20, 20),
                TileCoord::new(40, 40),
                4096,
            )),
        );
        w.remove_unit(id);
        assert!(w.collision_subscribers.errors.is_empty());
        assert!(w.collision_subscribers.memos.is_empty());
        assert!(w.collision_subscribers.planners.is_empty());
    }
}
