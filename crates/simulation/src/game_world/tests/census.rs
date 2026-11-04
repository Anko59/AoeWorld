use super::*;
use std::sync::Arc;

#[test]
fn census_reuses_a_100k_idle_report_across_ticks_and_movement() {
    let mut w = GameWorld::with_population(
        WorldConfig::new(4096, 4096, Seed(7)).unwrap(),
        100_000,
        0,
        1000,
    )
    .unwrap();
    for unit in &mut w.units {
        unit.state.stance = aoe_core::CombatStance::NoAttack;
    }
    let first = w.unit_census();
    assert_eq!(first.len(), 1000);
    assert_eq!(first.values().flat_map(|r| r.iter()).sum::<u32>(), 100_000);
    for _ in 0..4 {
        w.advance();
        assert!(Arc::ptr_eq(&first, &w.unit_census()));
    }
    let unit = w.units().next().unwrap();
    w.issue_stop(unit.player, unit.id).unwrap();
    assert!(Arc::ptr_eq(&first, &w.unit_census()));
    assert_eq!(w.unit_census_rebuilds(), 1);
}

#[test]
fn census_updates_birth_death_and_removal_without_rescanning() {
    let mut w = world();
    let empty = w.unit_census();
    let id = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Militia,
            WorldPosition::new(20_000, 20_000),
        )
        .unwrap();
    let live = w.unit_census();
    assert!(!Arc::ptr_eq(&empty, &live));
    assert_eq!(live[&PlayerId(0)][UnitKind::Militia as usize], 1);
    let hash = w.canonical_hash();
    assert!(Arc::ptr_eq(&live, &w.unit_census()));
    assert_eq!(hash, w.canonical_hash());
    w.kill_unit(w.lookup[&id]);
    let dead = w.unit_census();
    assert!(!Arc::ptr_eq(&live, &dead));
    assert!(dead.get(&PlayerId(0)).is_none());
    w.remove_unit(id);
    assert_eq!(*w.unit_census(), *dead);
    assert_eq!(w.unit_census_rebuilds(), 1);
}
