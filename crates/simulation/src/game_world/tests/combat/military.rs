//! Feudal values and interactions, independently pinned to the licensed DE dat.
use super::*;
use crate::{Age, BuildingKind, Stockpile, Tech, UnitKind, unit_combat};

pub(super) fn feudal(w: &mut GameWorld) {
    w.economy.players.insert(
        PlayerId(0),
        crate::PlayerEconomy {
            player: PlayerId(0),
            stockpile: Stockpile {
                food: 10000,
                wood: 10000,
                gold: 10000,
                stone: 10000,
            },
            population_used: 0,
            population_cap: 200,
            age: Age::Feudal,
            researched: Tech::FeudalAge.bit(),
            market_prices: crate::MarketPrices::default(),
        },
    );
}

#[test]
fn feudal_unit_values_costs_ranges_and_exact_speeds() {
    for (
        kind,
        hp,
        attack,
        pierce,
        range,
        min_range,
        period,
        speed,
        food,
        wood,
        gold,
        seconds,
        los,
    ) in [
        (UnitKind::Archer, 30, 4, 0, 4, 0, 200, 96, 0, 25, 45, 35, 6),
        (UnitKind::Scout, 45, 3, 2, 1, 0, 200, 120, 80, 0, 0, 30, 4),
        (
            UnitKind::Skirmisher,
            30,
            2,
            3,
            4,
            1,
            300,
            96,
            25,
            35,
            0,
            26,
            6,
        ),
        (
            UnitKind::Spearman,
            45,
            3,
            0,
            1,
            0,
            300,
            100,
            35,
            25,
            0,
            22,
            4,
        ),
        (
            UnitKind::ManAtArms,
            45,
            6,
            1,
            1,
            0,
            200,
            96,
            50,
            0,
            20,
            21,
            4,
        ),
    ] {
        let s = unit_combat(kind);
        assert_eq!(
            (s.hp, s.attack, s.melee_armour, s.pierce_armour),
            (hp, attack, 0, pierce)
        );
        assert_eq!(
            (
                s.range,
                s.min_range,
                s.attack_period_centiseconds,
                s.speed_hundredths,
                s.los
            ),
            (range, min_range, period, Some(speed), los)
        );
        let t = s.training.unwrap();
        assert_eq!(
            (t.cost.food, t.cost.wood, t.cost.gold, t.seconds),
            (food, wood, gold, seconds)
        );
    }
}

#[test]
fn spearman_bonus_kills_scout_in_three_hits() {
    let mut w = world();
    let spear = w
        .spawn_kind(PlayerId(0), UnitKind::Spearman, at(100, 100))
        .unwrap();
    let scout = w.spawn_unit(PlayerId(1), at(101, 100)).unwrap();
    w.issue_attack(PlayerId(0), spear, scout).unwrap();
    w.advance();
    assert_eq!(w.unit(scout).unwrap().hp, 27); // 3 melee + 15 cavalry class
    assert_eq!(
        ticks_until(&mut w, 121, |w| w.unit(scout).unwrap().hp == 0),
        120
    );
}

#[test]
fn bonus_class_damage_is_independent_of_pierce_armour() {
    let mut w = world();
    feudal(&mut w);
    w.economy.players.get_mut(&PlayerId(0)).unwrap().researched |= Tech::PaddedArcherArmor.bit();
    let victim = w
        .spawn_kind(PlayerId(0), UnitKind::Skirmisher, at(104, 100))
        .unwrap();
    let attacker = w
        .spawn_kind(PlayerId(1), UnitKind::Skirmisher, at(100, 100))
        .unwrap();
    w.issue_attack(PlayerId(1), attacker, victim).unwrap();
    advance(&mut w, 16);
    assert_eq!(
        w.unit(victim).unwrap().hp,
        27,
        "zero base pierce + three archer-class damage"
    );
}

#[test]
fn a_late_arrow_kill_does_not_cancel_a_new_attack_order() {
    let mut w = world();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(100, 100))
        .unwrap();
    let first = w.spawn_villager(PlayerId(1), at(104, 100)).unwrap();
    let second = w.spawn_villager(PlayerId(1), at(104, 101)).unwrap();
    w.units[w.lookup[&first]].state.hp = 1;
    w.issue_attack(PlayerId(0), a, first).unwrap();
    w.advance();
    w.issue_attack(PlayerId(0), a, second).unwrap();
    advance(&mut w, 12);
    assert_eq!(w.unit(first).unwrap().hp, 0);
    assert_eq!(w.unit(a).unwrap().attack_target, Some(second));
}

#[test]
fn ranged_attack_stays_at_range_and_damage_waits_for_travel() {
    let mut w = world();
    let archer = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(100, 100))
        .unwrap();
    let victim = w.spawn_villager(PlayerId(1), at(104, 100)).unwrap();
    w.issue_attack(PlayerId(0), archer, victim).unwrap();
    w.advance();
    assert_eq!(w.unit(victim).unwrap().hp, 25);
    assert_eq!(w.projectiles().count(), 1);
    assert_eq!(w.unit(archer).unwrap().position, at(100, 100));
    advance(&mut w, 12);
    assert_eq!(w.unit(victim).unwrap().hp, 21);
    assert_eq!(w.projectiles().count(), 0);
}

#[test]
fn skirmisher_archer_bonus_and_minimum_range() {
    let mut w = world();
    let skirm = w
        .spawn_kind(PlayerId(0), UnitKind::Skirmisher, at(100, 100))
        .unwrap();
    let archer = w
        .spawn_kind(PlayerId(1), UnitKind::Archer, at(104, 100))
        .unwrap();
    w.issue_attack(PlayerId(0), skirm, archer).unwrap();
    advance(&mut w, 16);
    assert_eq!(w.unit(archer).unwrap().hp, 25); // 2 pierce + 3 archer class
    let close = w
        .spawn_villager(
            PlayerId(1),
            WorldPosition::new(100 * 1024 + 768, 100 * 1024 + 512),
        )
        .unwrap();
    w.issue_attack(PlayerId(0), skirm, close).unwrap();
    advance(&mut w, 80);
    assert_eq!(w.unit(close).unwrap().hp, 25);
}

#[test]
fn unseen_targets_cannot_be_shot_and_projectile_projection_obeys_current_fog() {
    let mut w = world();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(100, 100))
        .unwrap();
    let b = w.spawn_villager(PlayerId(1), at(104, 100)).unwrap();
    w.issue_attack(PlayerId(0), a, b).unwrap();
    w.advance();
    assert_eq!(w.visible_projectiles(PlayerId(2)).count(), 0);
    assert_eq!(w.visible_projectiles(PlayerId(0)).count(), 1);
    w.update_observer(a, None);
    assert_eq!(w.visible_projectiles(PlayerId(0)).count(), 0);
    assert!(w.issue_attack(PlayerId(0), a, b).is_err());
}

#[test]
fn blacksmith_effects_are_selective_and_apply_to_future_units() {
    let mut w = world();
    feudal(&mut w);
    let mut ids = Vec::new();
    for (i, kind) in [
        UnitKind::Militia,
        UnitKind::Spearman,
        UnitKind::Scout,
        UnitKind::Archer,
        UnitKind::Skirmisher,
        UnitKind::Villager,
    ]
    .into_iter()
    .enumerate()
    {
        ids.push(
            w.spawn_kind(PlayerId(0), kind, at(100 + i as i32, 100))
                .unwrap(),
        );
    }
    for tech in [
        Tech::Forging,
        Tech::ScaleMailArmor,
        Tech::ScaleBardingArmor,
        Tech::Fletching,
        Tech::PaddedArcherArmor,
    ] {
        let mut changed = Vec::new();
        w.complete_research(PlayerId(0), tech, &mut changed);
    }
    for (i, (attack, armour, range)) in [
        (5, (1, 2), 1),
        (4, (1, 1), 1),
        (6, (1, 3), 1),
        (5, (1, 1), 5),
        (3, (1, 4), 5),
        (3, (0, 0), 1),
    ]
    .into_iter()
    .enumerate()
    {
        let stats = w.unit_stats(ids[i]).unwrap();
        assert_eq!(
            (stats.attack, w.unit_armour(ids[i]).unwrap(), stats.range),
            (attack, armour, range)
        );
    }
    let new = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(110, 100))
        .unwrap();
    assert_eq!(w.unit_stats(new).unwrap().attack, 5);
    for kind in [BuildingKind::TownCenter, BuildingKind::WatchTower] {
        assert_eq!(
            w.building_attack(PlayerId(0), kind).unwrap().range,
            if kind == BuildingKind::TownCenter {
                6
            } else {
                9
            }
        );
    }
}

#[test]
fn man_at_arms_upgrades_living_units_preserving_damage_and_future_training() {
    let mut w = world();
    feudal(&mut w);
    let id = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    w.units[w.lookup[&id]].state.hp = 30;
    w.complete_research(PlayerId(0), Tech::ManAtArms, &mut Vec::new());
    let u = w.unit(id).unwrap();
    assert_eq!((u.kind, u.hp, u.max_hp), (UnitKind::ManAtArms, 35, 45));
    let fresh = w.spawn_militia(PlayerId(0), at(110, 100)).unwrap();
    assert_eq!(w.unit(fresh).unwrap().kind, UnitKind::ManAtArms);
}

#[test]
fn production_queues_pay_refund_train_and_enforce_age_and_roster() {
    use crate::game_world::economy::placement::position;
    let mut w = world();
    w.initialize_village(PlayerId(0), TileCoord::new(60, 60))
        .unwrap();
    feudal(&mut w);
    for (i, (building, kinds)) in [
        (
            BuildingKind::Barracks,
            &[UnitKind::Militia, UnitKind::Spearman][..],
        ),
        (
            BuildingKind::ArcheryRange,
            &[UnitKind::Archer, UnitKind::Skirmisher][..],
        ),
        (BuildingKind::Stable, &[UnitKind::Scout][..]),
    ]
    .into_iter()
    .enumerate()
    {
        let x = 100 + i as i32 * 10;
        let rect = TileRect::new(TileCoord::new(x, 100), TileCoord::new(x + 3, 103));
        let site = w
            .insert_building(PlayerId(0), building, rect, true)
            .unwrap();
        w.economy
            .players
            .get_mut(&PlayerId(0))
            .unwrap()
            .population_cap = 200;
        for &kind in kinds {
            let before = w.player_economy(PlayerId(0)).unwrap().stockpile;
            w.issue_train(PlayerId(0), site, kind).unwrap();
            assert_eq!(w.building(site).unwrap().training_queue.len(), 1);
            w.issue_cancel_training(PlayerId(0), site).unwrap();
            assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, before);
            w.issue_train(PlayerId(0), site, kind).unwrap();
            let seconds = unit_combat(kind).training.unwrap().seconds;
            advance(&mut w, seconds * 20);
            assert!(w.building(site).unwrap().training_queue.is_empty());
            assert!(w.units().any(|u| u.kind == kind && u.position.x >= position(rect.min).unwrap().x - 1024));
        }
        assert!(
            w.issue_train(PlayerId(0), site, UnitKind::Villager)
                .is_err()
        );
        w.economy.players.get_mut(&PlayerId(0)).unwrap().age = Age::Dark;
        let kind = *kinds.last().unwrap();
        assert_eq!(
            w.issue_train(PlayerId(0), site, kind),
            Err(EconomyError::RequiresFeudalAge)
        );
        w.economy.players.get_mut(&PlayerId(0)).unwrap().age = Age::Feudal;
    }
}
