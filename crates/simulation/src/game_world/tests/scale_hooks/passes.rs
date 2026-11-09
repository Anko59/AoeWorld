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
    assert!(count(&counted, "collect_changed").visited > 0);
    assert!(count(&counted, "tables_refresh").calls > 0);
    assert_eq!(count(&counted, "advance_transports").visited, 0);
    // Replaced by events and indexes: these whole-population passes never run.
    for replaced in ["advance_gates", "remember_visible_buildings"] {
        assert_eq!(count(&counted, replaced), crate::PassCount::default());
    }
}

#[test]
fn a_collision_change_visits_its_subscribers_not_every_unit() {
    let mut w = busy_world();
    let far = w.spawn_unit(PlayerId(0), at(200, 200)).unwrap();
    let index = w.lookup[&far];
    w.set_movement_error(index, Some(crate::GameWorldError::Unreachable));
    w.remember_unreachable(index, TileCoord::new(5, 5));
    let area = TileRect::new(TileCoord::new(10, 10), TileCoord::new(12, 12));
    w.collision_closed(area);
    // A closing clears every error, wherever the unit stands, but no memo.
    assert_eq!(w.units[index].last_movement_error, None);
    assert!(w.units[index].unreachable_memo.is_some());
    w.collision_opened(area);
    assert_eq!(w.units[index].unreachable_memo, None);
    for replaced in ["collision_closed", "collision_opened"] {
        assert_eq!(count(&w, replaced), crate::PassCount::default());
    }
}
