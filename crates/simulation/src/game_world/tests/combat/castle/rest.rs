//! Resting healers wake on wounds and movement, including within one bucket.
use super::*;

fn pair(sleep: bool) -> GameWorld {
    let mut w = castle();
    w.set_work_sleep(sleep);
    w
}

fn tick(on: &mut GameWorld, off: &mut GameWorld, n: u32) {
    assert_eq!(on.advance(), off.advance(), "healing changes at {n}");
    assert_eq!(
        on.canonical_hash_hex(),
        off.canonical_hash_hex(),
        "healing hash at {n}"
    );
}

#[test]
fn healthy_monks_do_not_rescan_their_crowd_while_resting() {
    let (mut on, mut off) = (pair(true), pair(false));
    for w in [&mut on, &mut off] {
        for n in 0..32 {
            w.spawn_kind(PlayerId(0), UnitKind::Monk, at(100 + n % 8, 100 + n / 8))
                .unwrap();
        }
    }
    let (mut slept, mut awake) = (0, 0);
    for n in 0..240 {
        tick(&mut on, &mut off, n);
        slept += on.combat_stats().healing_scanned_units;
        awake += off.combat_stats().healing_scanned_units;
    }
    assert!(slept < 64, "resting healers scanned {slept} units");
    assert!(
        awake >= 900,
        "the ordinary healer loop must stay active: {awake}"
    );
}

#[test]
fn a_real_hit_wakes_resting_healers_on_the_original_heal_tick() {
    let (mut on, mut off) = (pair(true), pair(false));
    let mut patient = None;
    let mut enemy = None;
    for w in [&mut on, &mut off] {
        w.spawn_kind(PlayerId(0), UnitKind::Monk, at(100, 100))
            .unwrap();
        let id = w
            .spawn_kind(PlayerId(0), UnitKind::Knight, at(101, 100))
            .unwrap();
        w.issue_stance(PlayerId(0), &[id], aoe_core::CombatStance::NoAttack)
            .unwrap();
        patient = Some(id);
        enemy = Some(w.spawn_villager(PlayerId(1), at(80, 80)).unwrap());
    }
    for n in 0..300 {
        if n == 25 || n == 180 {
            for w in [&mut on, &mut off] {
                w.resolve_hit(
                    (enemy.unwrap(), PlayerId(1)),
                    patient.unwrap(),
                    8,
                    0,
                    DamageKind::Melee,
                    &mut Vec::new(),
                );
            }
        }
        tick(&mut on, &mut off, n);
    }
    let u = on.unit(patient.unwrap()).unwrap();
    assert_eq!(u.hp, u.max_hp);
}

#[test]
fn a_wounded_friend_entering_healing_range_within_one_bucket_wakes_monks() {
    let (mut on, mut off) = (pair(true), pair(false));
    let mut patient = None;
    for w in [&mut on, &mut off] {
        w.spawn_kind(PlayerId(0), UnitKind::Monk, at(100, 100))
            .unwrap();
        let id = w.spawn_villager(PlayerId(0), at(107, 100)).unwrap();
        w.units[w.lookup[&id]].state.hp -= 8;
        patient = Some(id);
    }
    for n in 0..500 {
        if n == 40 {
            for w in [&mut on, &mut off] {
                w.issue_move(patient.unwrap(), at(101, 100)).unwrap();
            }
        }
        tick(&mut on, &mut off, n);
    }
    let u = on.unit(patient.unwrap()).unwrap();
    assert_eq!(u.hp, u.max_hp);
}

#[test]
#[should_panic(expected = "missed wake: healer")]
fn healer_detector_rejects_an_unannounced_wound() {
    let mut w = pair(true);
    w.spawn_kind(PlayerId(0), UnitKind::Monk, at(100, 100))
        .unwrap();
    let patient = w.spawn_villager(PlayerId(0), at(101, 100)).unwrap();
    for _ in 0..16 {
        w.advance();
    }
    w.units[w.lookup[&patient]].state.hp -= 1;
    for _ in 0..8 {
        w.advance();
    }
}

#[test]
fn healthy_walkers_do_not_wake_a_resting_healer() {
    let (mut on, mut off) = (pair(true), pair(false));
    let mut walker = None;
    for w in [&mut on, &mut off] {
        w.spawn_kind(PlayerId(0), UnitKind::Monk, at(100, 100))
            .unwrap();
        walker = Some(w.spawn_villager(PlayerId(0), at(101, 100)).unwrap());
    }
    let mut scanned = 0;
    for n in 0..240 {
        if n == 20 {
            for w in [&mut on, &mut off] {
                w.issue_move(walker.unwrap(), at(105, 100)).unwrap();
            }
        }
        tick(&mut on, &mut off, n);
        scanned += on.combat_stats().healing_scanned_units;
    }
    assert!(scanned < 8, "healthy motion woke a healer: {scanned}");
}
