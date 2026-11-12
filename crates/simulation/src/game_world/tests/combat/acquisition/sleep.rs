//! Empty threat regions sleep without changing rotation or shared budgets.
use super::*;

fn quiet(sleep: bool, count: i32) -> (GameWorld, Vec<EntityId>) {
    let mut w = world();
    w.set_work_sleep(sleep);
    let ids = (0..count)
        .map(|n| {
            w.spawn_militia(PlayerId(0), at(100 + n % 8, 100 + n / 8))
                .unwrap()
        })
        .collect();
    (w, ids)
}

fn pair_tick(on: &mut GameWorld, off: &mut GameWorld, tick: u32) {
    assert_eq!(on.advance(), off.advance(), "acquisition changes at {tick}");
    assert_eq!(
        on.canonical_hash_hex(),
        off.canonical_hash_hex(),
        "acquisition hash at {tick}"
    );
}

#[test]
fn an_idle_army_without_threats_stops_visiting_its_units() {
    let (mut on, _) = quiet(true, 64);
    let (mut off, _) = quiet(false, 64);
    let mut visits = 0;
    for tick in 0..240 {
        pair_tick(&mut on, &mut off, tick);
        visits += on.combat_stats().acquisition_checks;
    }
    assert!(visits < 128, "quiet army made {visits} unit visits");
    assert_eq!(off.combat_stats().acquisition_checks, 64);
}

#[test]
fn an_enemy_entering_a_sleeping_region_wakes_the_same_rotation_turn() {
    let (mut on, ids) = quiet(true, 16);
    let (mut off, _) = quiet(false, 16);
    let mut target = None;
    for tick in 0..600 {
        if tick == 100 {
            for w in [&mut on, &mut off] {
                let enemy = w.spawn_villager(PlayerId(1), at(128, 100)).unwrap();
                w.issue_move(enemy, at(105, 100)).unwrap();
                target = Some(enemy);
            }
        }
        pair_tick(&mut on, &mut off, tick);
    }
    assert!(on.unit(target.unwrap()).is_none_or(|u| u.hp < u.max_hp));
    assert!(ids.iter().any(|&id| on.unit(id).is_some()));
}

#[test]
fn a_sleeping_source_wakes_on_orders_and_stance_changes() {
    let (mut on, ids) = quiet(true, 1);
    let (mut off, _) = quiet(false, 1);
    for tick in 0..500 {
        for w in [&mut on, &mut off] {
            if tick == 50 {
                w.issue_move(ids[0], at(110, 100)).unwrap();
            }
            if tick == 100 {
                w.issue_stop(PlayerId(0), ids[0]).unwrap();
                w.issue_stance(PlayerId(0), &ids, aoe_core::CombatStance::NoAttack)
                    .unwrap();
            }
            if tick == 150 {
                w.spawn_villager(PlayerId(1), at(106, 100)).unwrap();
            }
            if tick == 200 {
                w.issue_stance(PlayerId(0), &ids, aoe_core::CombatStance::Aggressive)
                    .unwrap();
            }
        }
        pair_tick(&mut on, &mut off, tick);
    }
}

#[test]
fn new_hostile_buildings_wake_sleeping_acquisition() {
    let (mut on, _) = quiet(true, 1);
    let (mut off, _) = quiet(false, 1);
    let mut target = None;
    for tick in 0..300 {
        if tick == 80 {
            for w in [&mut on, &mut off] {
                target = Some(
                    w.insert_building(
                        PlayerId(1),
                        BuildingKind::House,
                        TileRect::from_xywh(103, 100, 2, 2),
                        true,
                    )
                    .unwrap(),
                );
            }
        }
        pair_tick(&mut on, &mut off, tick);
    }
    assert!(on.building(target.unwrap()).is_none_or(|b| b.hp < b.max_hp));
}

#[test]
fn sleeping_dense_friends_preserve_the_budget_of_an_active_battle() {
    let (mut on, _) = quiet(true, 192);
    let (mut off, _) = quiet(false, 192);
    for w in [&mut on, &mut off] {
        for n in 0..48 {
            w.spawn_militia(PlayerId(0), at(184 + n % 8, 180 + n / 8))
                .unwrap();
            w.spawn_militia(PlayerId(1), at(194 + n % 8, 180 + n / 8))
                .unwrap();
        }
    }
    let mut slept = 0;
    for tick in 0..800 {
        pair_tick(&mut on, &mut off, tick);
        slept += on.combat_stats().acquisition_slept;
    }
    assert!(slept > 10_000, "the quiet colony must sleep during battle");
}

#[test]
#[should_panic(expected = "missed wake: seeker")]
fn the_acquisition_detector_rejects_an_unannounced_source_change() {
    let (mut w, ids) = quiet(true, 1);
    w.advance();
    w.units[w.lookup[&ids[0]]].state.stance = aoe_core::CombatStance::NoAttack;
    w.advance();
}

#[test]
fn no_attack_units_wait_for_a_stance_event_even_when_an_enemy_is_near() {
    let (mut on, ids) = quiet(true, 16);
    let (mut off, _) = quiet(false, 16);
    let mut enemy = None;
    for w in [&mut on, &mut off] {
        w.issue_stance(PlayerId(0), &ids, aoe_core::CombatStance::NoAttack)
            .unwrap();
        enemy = Some(w.spawn_villager(PlayerId(1), at(110, 100)).unwrap());
    }
    let mut visits = 0;
    for n in 0..300 {
        if n == 100 {
            let u = on.unit(enemy.unwrap()).unwrap();
            assert_eq!(u.hp, u.max_hp, "No Attack must hold fire");
            for w in [&mut on, &mut off] {
                w.issue_stance(PlayerId(0), &ids, aoe_core::CombatStance::Aggressive)
                    .unwrap();
            }
        }
        pair_tick(&mut on, &mut off, n);
        if n < 100 {
            visits += on.combat_stats().acquisition_checks;
        }
    }
    assert!(
        visits < 32,
        "disabled acquisition made {visits} unit visits"
    );
    assert!(on.unit(enemy.unwrap()).is_none_or(|u| u.hp < u.max_hp));
}

#[test]
fn a_retained_enemy_corpse_does_not_keep_an_idle_fighter_awake() {
    let (mut on, _) = quiet(true, 1);
    let (mut off, _) = quiet(false, 1);
    let mut enemy = None;
    for w in [&mut on, &mut off] {
        enemy = Some(w.spawn_villager(PlayerId(1), at(102, 100)).unwrap());
    }
    let mut visits = 0;
    for n in 0..180 {
        if n == 20 {
            for w in [&mut on, &mut off] {
                w.issue_delete(PlayerId(1), &[enemy.unwrap()]).unwrap();
            }
        }
        pair_tick(&mut on, &mut off, n);
        if n >= 60 {
            visits += on.combat_stats().acquisition_checks;
        }
    }
    assert_eq!(
        on.unit(enemy.unwrap()).unwrap().hp,
        0,
        "the corpse must still be resident"
    );
    assert!(
        visits < 3,
        "a corpse kept acquisition awake for {visits} turns"
    );
}
