//! Unordered armies defend their owners without a controller issuing targets.
use super::*;
use crate::{BuildingKind, UnitKind};
mod obstacles;
mod passing;
mod sleep;

#[test]
fn far_pursuit_budget_does_not_starve_later_units_when_the_target_keeps_moving() {
    let mut w = world();
    w.set_fog_disabled(true);
    let ids = (0..128)
        .map(|n| {
            w.spawn_militia(PlayerId(0), at(10 + n % 16 * 4, 10 + n / 16 * 4))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let target = w.spawn_unit(PlayerId(1), at(200, 200)).unwrap();
    w.issue_move(target, at(240, 200)).unwrap();
    for &id in &ids {
        w.issue_attack(PlayerId(0), id, target).unwrap();
    }
    let mut routed = std::collections::BTreeSet::new();
    for _ in 0..40 {
        w.advance();
        for &id in &ids {
            if w.movement_order(id).is_some() {
                routed.insert(id);
            }
        }
    }
    assert_eq!(
        routed.len(),
        ids.len(),
        "every accepted pursuer needs a turn"
    );
}

#[test]
fn an_arrived_formation_acquires_enemies_after_its_move_finishes() {
    let mut w = world();
    let a = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let b = w.spawn_militia(PlayerId(0), at(102, 100)).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &[a, b],
        aoe_core::Formation::Line,
        at(110, 100),
        false,
        false,
    )
    .unwrap();
    ticks_until(&mut w, 500, |w| {
        w.formation_waypoints(a)
            .is_some_and(|orders| orders.is_empty())
    });
    let enemy = w
        .spawn_villager(PlayerId(1), w.unit(a).unwrap().position)
        .unwrap();
    ticks_until(&mut w, 20, |w| {
        w.unit(a).unwrap().attack_target == Some(enemy)
    });
    assert!(w.formation(a).is_none());
}
#[test]
fn an_automatic_building_attack_yields_to_units_but_a_manual_target_is_kept() {
    for manual in [false, true] {
        let mut w = world();
        let a = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
        let house = w
            .insert_building(
                PlayerId(1),
                BuildingKind::House,
                TileRect::from_xywh(102, 100, 2, 2),
                true,
            )
            .unwrap();
        if manual {
            w.issue_attack(PlayerId(0), a, house).unwrap();
        }
        // An idle soldier looks on its own turn, once a second (20 ticks).
        advance(&mut w, 20);
        assert_eq!(w.unit(a).unwrap().attack_target, Some(house));
        let enemy = w.spawn_villager(PlayerId(1), at(100, 102)).unwrap();
        advance(&mut w, 20);
        assert_eq!(
            w.unit(a).unwrap().attack_target,
            Some(if manual { house } else { enemy })
        );
    }
}

#[test]
fn an_attack_burst_routes_every_attacker_at_once_and_repeated_targets_reuse_orders() {
    let mut w = world();
    let target = w.spawn_villager(PlayerId(1), at(200, 200)).unwrap();
    let ids = (0..32)
        .map(|n| w.spawn_militia(PlayerId(0), at(10 + n, 10)).unwrap())
        .collect::<Vec<_>>();
    for &id in &ids {
        w.issue_attack(PlayerId(0), id, target).unwrap();
    }
    assert_eq!(w.combat.attackers.len(), 32);
    assert!(
        ids.iter().all(|id| w.movement_order(*id).is_some()),
        "no attacker waits for another's search"
    );
    let before = w.combat.stats.attack_searches;
    for &id in &ids {
        w.issue_attack(PlayerId(0), id, target).unwrap();
    }
    assert_eq!(w.combat.stats.attack_searches, before);
}

/// A large order searches in the combat phase, side by side, not one unit
/// after another inside the command; every attacker is routed that tick, to
/// the same spots as orders issued one by one (each reserves its spot first).
#[test]
fn a_large_attack_order_searches_in_the_combat_phase_and_routes_everyone() {
    let army = |w: &mut GameWorld| {
        let target = w.spawn_villager(PlayerId(1), at(60, 60)).unwrap();
        let ids = (0..600)
            .map(|n| {
                w.spawn_militia(PlayerId(0), at(10 + n % 30, 10 + n / 30))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        (target, ids)
    };
    let destinations = |w: &GameWorld, ids: &[EntityId]| {
        ids.iter()
            .map(|id| w.movement_order(*id).map(|o| o.destination))
            .collect::<Vec<_>>()
    };
    let mut one_by_one = world();
    let (target, ids) = army(&mut one_by_one);
    for &id in &ids {
        one_by_one.issue_attack(PlayerId(0), id, target).unwrap();
    }
    let mut w = world();
    let (target, ids) = army(&mut w);
    let before = w.combat.stats.attack_searches;
    w.issue_player_attack(PlayerId(0), &ids, target).unwrap();
    assert_eq!(
        w.combat.stats.attack_searches, before,
        "no search in the command"
    );
    assert!(
        ids.iter()
            .all(|id| w.unit(*id).unwrap().attack_target == Some(target))
    );
    w.advance();
    let routed = destinations(&w, &ids);
    assert!(
        routed.iter().all(Option::is_some),
        "every attacker is routed in the first combat phase"
    );
    assert_eq!(
        routed,
        destinations(&one_by_one, &ids),
        "same spots as one by one"
    );
}

#[test]
fn no_attack_suppresses_acquisition_and_retaliation_but_explicit_orders_work() {
    use aoe_core::CombatStance;
    let (mut w, a, b) = duel();
    w.issue_stance(PlayerId(0), &[a], CombatStance::NoAttack)
        .unwrap();
    advance(&mut w, 10);
    assert_eq!(w.unit(a).unwrap().attack_target, None);
    w.issue_attack(PlayerId(1), b, a).unwrap();
    w.advance();
    assert_eq!(w.unit(a).unwrap().attack_target, None);
    w.issue_attack(PlayerId(0), a, b).unwrap();
    w.advance();
    assert!(w.unit(b).unwrap().last_hit.is_some());
}

#[test]
fn stand_ground_only_acquires_inside_weapon_range_and_never_chases() {
    use aoe_core::CombatStance;
    let mut w = world();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(100, 100))
        .unwrap();
    let b = w
        .spawn_kind(PlayerId(1), UnitKind::Scout, at(105, 100))
        .unwrap();
    w.issue_stance(PlayerId(0), &[a], CombatStance::StandGround)
        .unwrap();
    w.issue_stance(PlayerId(1), &[b], CombatStance::NoAttack)
        .unwrap();
    advance(&mut w, 10);
    assert_eq!(w.unit(a).unwrap().attack_target, None);
    w.issue_move(b, at(103, 100)).unwrap();
    ticks_until(&mut w, 100, |w| w.unit(a).unwrap().attack_target == Some(b));
    w.issue_move(b, at(110, 100)).unwrap();
    advance(&mut w, 100);
    assert_eq!(w.unit(a).unwrap().position, at(100, 100));
    assert_eq!(w.unit(a).unwrap().attack_target, None);
}

#[test]
fn defensive_pursuit_returns_to_its_origin_and_stance_groups_are_atomic() {
    use aoe_core::CombatStance;
    let mut w = world();
    let a = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let b = w.spawn_unit(PlayerId(1), at(103, 100)).unwrap();
    assert!(
        w.issue_stance(PlayerId(0), &[a, b], CombatStance::NoAttack)
            .is_err()
    );
    assert_eq!(w.unit(a).unwrap().stance, CombatStance::Aggressive);
    w.issue_stance(PlayerId(0), &[a], CombatStance::Defensive)
        .unwrap();
    w.issue_stance(PlayerId(1), &[b], CombatStance::NoAttack)
        .unwrap();
    advance(&mut w, 20);
    assert_eq!(w.unit(a).unwrap().attack_target, Some(b));
    w.issue_move(b, at(115, 100)).unwrap();
    advance(&mut w, 400);
    assert_eq!(w.unit(a).unwrap().position, at(100, 100));
    assert_eq!(w.unit(a).unwrap().attack_target, None);
}

#[test]
fn dense_crowds_have_bounded_target_work_and_cannot_hide_an_enemy_forever() {
    use aoe_core::CombatStance;
    let mut w = world();
    for _ in 0..300 {
        let id = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
        w.issue_stance(PlayerId(0), &[id], CombatStance::NoAttack)
            .unwrap();
    }
    let a = w.spawn_militia(PlayerId(0), at(101, 100)).unwrap();
    let enemy = w.spawn_villager(PlayerId(1), at(102, 100)).unwrap();
    for _ in 0..50 {
        w.advance();
        let stats = w.combat_stats();
        assert!(stats.attack_searches <= 16);
        if w.unit(a).unwrap().attack_target == Some(enemy) {
            return;
        }
    }
    panic!("rotation never inspected the enemy");
}

#[test]
fn an_idle_villager_flees_ranged_fire_without_dropping_its_load() {
    let mut w = world();
    let archer = w
        .spawn_kind(PlayerId(1), UnitKind::Archer, at(104, 100))
        .unwrap();
    let villager = w.spawn_villager(PlayerId(0), at(100, 100)).unwrap();
    let load = crate::CarriedResource {
        kind: aoe_map::ResourceKind::Wood,
        amount: 7,
    };
    w.units[w.lookup[&villager]].state.carried = Some(load);
    w.issue_attack(PlayerId(1), archer, villager).unwrap();
    ticks_until(&mut w, 30, |w| w.unit(villager).unwrap().last_hit.is_some());
    assert_eq!(w.unit(villager).unwrap().attack_target, None);
    assert!(w.movement_order(villager).is_some());
    assert_eq!(w.unit(villager).unwrap().carried, Some(load));
}

#[test]
fn idle_military_acquires_a_unit_before_a_closer_building() {
    let mut w = world();
    let soldier = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    w.insert_building(
        PlayerId(1),
        BuildingKind::House,
        TileRect::from_xywh(102, 100, 2, 2),
        true,
    )
    .unwrap();
    let enemy = w.spawn_villager(PlayerId(1), at(103, 102)).unwrap();
    advance(&mut w, 20);
    assert_eq!(w.unit(soldier).unwrap().attack_target, Some(enemy));
}

#[test]
fn acquisition_uses_own_sight_and_does_not_interrupt_a_move_or_worker() {
    let mut w = world();
    let soldier = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let worker = w.spawn_villager(PlayerId(0), at(101, 100)).unwrap();
    let enemy = w.spawn_militia(PlayerId(1), at(109, 100)).unwrap();
    w.issue_stance(PlayerId(1), &[enemy], aoe_core::CombatStance::NoAttack)
        .unwrap();
    // Team sight alone cannot make the militia acquire beyond its own LOS.
    w.spawn_villager(PlayerId(0), at(108, 100)).unwrap();
    advance(&mut w, 10);
    assert_eq!(w.unit(soldier).unwrap().attack_target, None);
    assert_eq!(w.unit(worker).unwrap().attack_target, None);
    w.issue_move(soldier, at(120, 100)).unwrap();
    advance(&mut w, 10);
    assert_eq!(w.unit(soldier).unwrap().attack_target, None);
    assert_eq!(w.unit(enemy).unwrap().hp, w.unit(enemy).unwrap().max_hp);
}

#[test]
fn an_idle_soldier_retaliates_against_a_building_attacker() {
    let mut w = world();
    let tower = w
        .insert_building(
            PlayerId(1),
            BuildingKind::WatchTower,
            TileRect::from_xywh(110, 100, 1, 1),
            true,
        )
        .unwrap();
    let soldier = w.spawn_militia(PlayerId(0), at(104, 100)).unwrap();
    // Outside own LOS: this target comes from being hit, not acquisition.
    ticks_until(&mut w, 100, |w| w.unit(soldier).unwrap().last_hit.is_some());
    assert_eq!(w.unit(soldier).unwrap().attack_target, Some(tower));
}

#[test]
fn an_ai_owned_idle_army_defends_without_python_orders() {
    let mut w = world();
    let ai = w.spawn_militia(PlayerId(37), at(100, 100)).unwrap();
    let enemy = w.spawn_militia(PlayerId(0), at(101, 100)).unwrap();
    advance(&mut w, 20);
    assert_eq!(w.unit(ai).unwrap().attack_target, Some(enemy));
    assert!(w.unit(enemy).unwrap().hp < w.unit(enemy).unwrap().max_hp);
    assert!(w.unit(ai).unwrap().hp < w.unit(ai).unwrap().max_hp);
}

#[test]
fn a_castle_accepts_a_manual_building_target_and_fires_at_it() {
    let mut w = world();
    let castle = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(100, 100, 4, 4),
            true,
        )
        .unwrap();
    let target = w
        .insert_building(
            PlayerId(1),
            BuildingKind::House,
            TileRect::from_xywh(107, 100, 2, 2),
            true,
        )
        .unwrap();
    let hp = w.building(target).unwrap().hp;
    w.issue_attack(PlayerId(0), castle, target).unwrap();
    ticks_until(&mut w, 100, |w| w.building(target).unwrap().hp < hp);
    assert_eq!(
        w.building(castle).unwrap().hp,
        w.building(castle).unwrap().max_hp
    );
}

#[test]
fn a_castle_fires_at_an_approaching_unit_without_orders() {
    let mut w = world();
    w.insert_building(
        PlayerId(0),
        BuildingKind::Castle,
        TileRect::from_xywh(100, 100, 4, 4),
        true,
    )
    .unwrap();
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(115, 102))
        .unwrap();
    w.issue_move(enemy, at(108, 102)).unwrap();
    ticks_until(&mut w, 300, |w| w.unit(enemy).unwrap().hp < 100);
}

/// Far more idle soldiers than the former world-wide ration of 128 looks per
/// tick: all have a target within a second (20 ticks), the idle look turn.
#[test]
fn every_idle_soldier_finds_a_target_within_its_one_second_turn() {
    use aoe_core::CombatStance;
    let mut w = world();
    w.set_fog_disabled(true);
    let archers: Vec<_> = (0..600)
        .map(|_| {
            w.spawn_kind(PlayerId(0), UnitKind::Archer, at(100, 100))
                .unwrap()
        })
        .collect();
    let enemy = w.spawn_villager(PlayerId(1), at(103, 100)).unwrap();
    w.units[w.lookup[&enemy]].state.stance = CombatStance::NoAttack;
    // It must outlive the second, so every archer's look can find it.
    w.units[w.lookup[&enemy]].state.max_hp = u16::MAX;
    w.units[w.lookup[&enemy]].state.hp = u16::MAX;
    let mut turn = std::collections::BTreeMap::new();
    for tick in 1..=20_u64 {
        w.advance();
        for &id in &archers {
            if w.unit(id).unwrap().attack_target == Some(enemy) {
                turn.entry(id).or_insert(tick);
            }
        }
    }
    // Spawned with nobody near, the archers start asleep; the enemy's spawn
    // wakes them and each looks on its idle turn within the second.
    assert_eq!(turn.len(), archers.len());
}
