//! DE elevation, fractional HP, and primary/secondary arrow rules.
use super::*;
use crate::{BuildingKind, UnitKind};

#[test]
fn hill_fractional_health_survives_shelter_and_healing_caps_at_maximum() {
    let mut w = world();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(104, 100))
        .unwrap();
    let v = w.spawn_villager(PlayerId(1), at(100, 100)).unwrap();
    let home = w
        .insert_building(
            PlayerId(1),
            BuildingKind::TownCenter,
            TileRect::from_xywh(96, 100, 4, 4),
            true,
        )
        .unwrap();
    quiet(&mut w, a);
    hills(&mut w, true);
    hit(&mut w, a, v, 1, 0, DamageKind::Pierce);
    w.issue_garrison(PlayerId(1), v, home).unwrap();
    advance(&mut w, 2);
    assert!(w.unit(v).is_none());
    assert_eq!(w.building(home).unwrap().garrison[0].unit.hp, 23);
    assert_eq!(w.combat.hp_quarters[v.0 as usize], 3);
    advance(&mut w, 200);
    assert_eq!(w.building(home).unwrap().garrison[0].unit.hp, 24);
    assert_eq!(w.combat.hp_quarters[v.0 as usize], 3);
    advance(&mut w, 200);
    assert_eq!(w.building(home).unwrap().garrison[0].unit.hp, 25);
    assert_eq!(w.combat.hp_quarters[v.0 as usize], 0);
}

#[test]
fn hill_fractional_health_is_capped_when_a_monk_finishes_healing() {
    let mut w = world();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(104, 100))
        .unwrap();
    let v = w.spawn_villager(PlayerId(1), at(100, 100)).unwrap();
    w.spawn_kind(PlayerId(1), UnitKind::Monk, at(100, 102))
        .unwrap();
    quiet(&mut w, a);
    hills(&mut w, true);
    hit(&mut w, a, v, 1, 0, DamageKind::Pierce);
    assert_eq!(w.combat.hp_quarters[v.0 as usize], 3);
    advance(&mut w, 16);
    assert_eq!(w.unit(v).unwrap().hp, 25);
    assert_eq!(w.combat.hp_quarters[v.0 as usize], 0);
}

#[test]
fn hill_fractional_health_is_replay_state_even_when_displayed_hp_matches() {
    let mut w = world();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(104, 100))
        .unwrap();
    let v = w.spawn_villager(PlayerId(1), at(100, 100)).unwrap();
    hills(&mut w, true);
    hit(&mut w, a, v, 1, 0, DamageKind::Pierce);
    let fractional = w.canonical_hash();
    let displayed_hp = w.unit(v).unwrap().hp;
    w.combat.hp_quarters[v.0 as usize] = 0;
    assert_eq!(w.unit(v).unwrap().hp, displayed_hp);
    assert_ne!(w.canonical_hash(), fractional);
}

fn hills(w: &mut GameWorld, rising: bool) {
    use aoe_map::*;
    let (mut p, mut pages, water) = super::super::economy::fixture::flat_fixture(false);
    p.request.requested_side_meters = 15_360;
    if rising {
        pages[0].geographic_height_centimeters = vec![0, 768_000, 0, 768_000];
    }
    for (level, page) in p.environment.elevation.levels.iter_mut().zip(&pages) {
        level.ordered_page_root = ordered_page_root(std::slice::from_ref(page)).unwrap();
    }
    let p = MapPackage::with_prepared_environment(
        1,
        p.request,
        Vec::new(),
        p.projection,
        p.provenance,
        p.environment,
    )
    .unwrap();
    w.terrain =
        crate::terrain::Terrain::from_prepared_package(&p, pages, water, Vec::new(), Vec::new())
            .unwrap();
}

fn hit(
    w: &mut GameWorld,
    attacker: EntityId,
    target: EntityId,
    attack: u16,
    bonus: u16,
    kind: DamageKind,
) {
    w.resolve_hit(
        (attacker, PlayerId(0)),
        target,
        attack,
        bonus,
        kind,
        &mut Vec::new(),
    );
}

fn quiet(w: &mut GameWorld, id: EntityId) {
    let player = w.unit(id).unwrap().player;
    w.issue_stance(player, &[id], aoe_core::CombatStance::NoAttack)
        .unwrap();
}

#[test]
fn hill_multiplier_follows_armour_and_bonus_and_depends_only_on_height_order() {
    for (ax, vx, rising, expected) in [
        (104, 100, true, 70),
        (100, 104, true, 82),
        (100, 104, false, 76),
    ] {
        let mut w = world();
        let a = w
            .spawn_kind(PlayerId(0), UnitKind::Pikeman, at(ax, 100))
            .unwrap();
        let v = w
            .spawn_kind(PlayerId(1), UnitKind::Knight, at(vx, 100))
            .unwrap();
        hills(&mut w, rising);
        hit(&mut w, a, v, 4, 22, DamageKind::Melee);
        assert_eq!(w.unit(v).unwrap().hp, expected); // (4 - 2 + 22) × 5/4, 3/4, or 1.
    }
}

#[test]
fn hill_fractional_damage_accumulates_and_the_minimum_is_applied_last() {
    let mut w = world();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(104, 100))
        .unwrap();
    let v = w.spawn_villager(PlayerId(1), at(100, 100)).unwrap();
    let ram = w
        .spawn_kind(PlayerId(1), UnitKind::BatteringRam, at(100, 102))
        .unwrap();
    let uphill = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(100, 104))
        .unwrap();
    let high = w.spawn_villager(PlayerId(1), at(104, 104)).unwrap();
    hills(&mut w, true);
    for hp in [23, 22, 21, 20] {
        hit(&mut w, a, v, 1, 0, DamageKind::Pierce);
        assert_eq!(w.unit(v).unwrap().hp, hp); // DE retains the 1.25 HP damage between hits.
    }
    for hp in [174, 173, 172, 171] {
        hit(&mut w, a, ram, 1, 0, DamageKind::Pierce);
        assert_eq!(w.unit(ram).unwrap().hp, hp); // max(1, 0 × 1.25), not 1 × 1.25.
    }
    for hp in [24, 23, 22, 21] {
        hit(&mut w, uphill, high, 1, 0, DamageKind::Pierce);
        assert_eq!(w.unit(high).unwrap().hp, hp); // max(1, 1 × 0.75).
    }
}

#[test]
fn hill_multiplier_applies_to_buildings_receiving_unit_attacks() {
    for (ax, bx, expected) in [(104, 100, 541), (100, 104, 544)] {
        let mut w = world();
        let a = w
            .spawn_kind(PlayerId(0), UnitKind::Pikeman, at(ax, 100))
            .unwrap();
        let b = w
            .insert_building(
                PlayerId(1),
                BuildingKind::House,
                TileRect::from_xywh(bx, 100, 2, 2),
                true,
            )
            .unwrap();
        hills(&mut w, true);
        hit(&mut w, a, b, 4, 1, DamageKind::Melee);
        assert_eq!(w.building(b).unwrap().hp, expected); // Seven base + class damage, then elevation.
    }
}

#[test]
fn hill_projectile_from_a_dead_shooter_has_no_elevation_multiplier() {
    let mut w = world();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(104, 100))
        .unwrap();
    let v = w.spawn_villager(PlayerId(1), at(100, 100)).unwrap();
    hills(&mut w, true);
    w.issue_attack(PlayerId(0), a, v).unwrap();
    w.advance();
    assert_eq!(w.projectiles().count(), 1);
    w.kill_unit(w.lookup[&a]);
    advance(&mut w, 20);
    assert_eq!(w.unit(v).unwrap().hp, 21);
}

#[test]
fn secondary_castle_arrows_skip_hills_and_the_primary_spearman_bonus() {
    for (victim, rising, expected) in [
        (UnitKind::Knight, true, 952), // 9 × 1.25 + four secondary arrows of 9.
        (UnitKind::Spearman, false, 943), // 13 primary + four secondary arrows of 11.
    ] {
        let mut w = world();
        let b = w
            .insert_building(
                PlayerId(0),
                BuildingKind::Castle,
                TileRect::from_xywh(104, 100, 4, 4),
                true,
            )
            .unwrap();
        let v = w.spawn_kind(PlayerId(1), victim, at(100, 100)).unwrap();
        w.units[w.lookup[&v]].state.hp = 1000;
        w.units[w.lookup[&v]].state.max_hp = 1000;
        quiet(&mut w, v);
        hills(&mut w, rising);
        w.issue_attack(PlayerId(0), b, v).unwrap();
        advance(&mut w, 25);
        assert_eq!(w.unit(v).unwrap().hp, expected);
    }
}

#[test]
fn secondary_town_center_arrows_all_ignore_hills() {
    let mut w = world();
    let b = w
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            TileRect::from_xywh(104, 100, 4, 4),
            true,
        )
        .unwrap();
    let g = w.spawn_villager(PlayerId(0), at(103, 100)).unwrap();
    w.issue_garrison(PlayerId(0), g, b).unwrap();
    advance(&mut w, 2);
    assert_eq!(w.building_arrow_count(b), 1);
    let v = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(100, 100))
        .unwrap();
    quiet(&mut w, v);
    hills(&mut w, true);
    w.issue_attack(PlayerId(0), b, v).unwrap();
    advance(&mut w, 25);
    assert_eq!(w.unit(v).unwrap().hp, 97); // 5 pierce - 2 armour, no hill multiplier.
}

#[test]
fn secondary_tower_arrows_have_their_own_stone_defense_attack() {
    let mut w = world();
    let b = w
        .insert_building(
            PlayerId(0),
            BuildingKind::WatchTower,
            TileRect::from_xywh(104, 100, 1, 1),
            true,
        )
        .unwrap();
    let g = w.spawn_villager(PlayerId(0), at(103, 100)).unwrap();
    w.issue_garrison(PlayerId(0), g, b).unwrap();
    advance(&mut w, 2);
    assert_eq!(w.building_arrow_count(b), 2);
    let target = w
        .insert_building(
            PlayerId(1),
            BuildingKind::WatchTower,
            TileRect::from_xywh(100, 100, 1, 1),
            true,
        )
        .unwrap();
    let hp = w.building(target).unwrap().hp;
    w.issue_attack(PlayerId(0), b, target).unwrap();
    advance(&mut w, 20);
    assert_eq!(hp - w.building(target).unwrap().hp, 6); // Minimum primary 1 + Stone Defense 5.
}

/// A complete player-1 house on the rising side, an archer uphill, and a player-1 builder.
fn hilltop_house() -> (GameWorld, EntityId, EntityId, EntityId) {
    let mut w = world();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Pikeman, at(104, 100))
        .unwrap();
    let builder = w.spawn_villager(PlayerId(1), at(99, 100)).unwrap();
    let house = w
        .insert_building(
            PlayerId(1),
            BuildingKind::House,
            TileRect::from_xywh(100, 100, 2, 2),
            true,
        )
        .unwrap();
    w.economy.players.insert(
        PlayerId(1),
        crate::PlayerEconomy {
            player: PlayerId(1),
            stockpile: crate::Stockpile {
                food: 10000,
                wood: 10000,
                gold: 10000,
                stone: 10000,
            },
            population_used: 0,
            age: crate::Age::Feudal,
            researched: crate::Tech::FeudalAge.bit(),
            market_prices: crate::MarketPrices::default(),
        },
    );
    quiet(&mut w, a);
    hills(&mut w, true);
    (w, a, house, builder)
}

#[test]
fn repairing_to_full_clears_fractional_hp_so_the_next_hit_matches_a_fresh_building() {
    let (mut fresh, a, house, _) = hilltop_house();
    let full = fresh.building(house).unwrap().hp;
    hit(&mut fresh, a, house, 4, 1, DamageKind::Melee);
    let fresh_loss = full - fresh.building(house).unwrap().hp;

    let (mut w, a, house, builder) = hilltop_house();
    hit(&mut w, a, house, 4, 1, DamageKind::Melee);
    assert_ne!(
        w.combat.hp_quarters[house.0 as usize], 0,
        "the first hit must leave fractional HP"
    );
    // The hill map blocks the builder path, so repair runs on open ground and the hill returns for the final hit.
    w.terrain = world().terrain;
    w.issue_construct(PlayerId(1), &[builder], house).unwrap();
    ticks_until(&mut w, 2_000, |w| {
        w.building(house).is_some_and(|b| b.hp == b.max_hp)
    });
    assert_eq!(w.combat.hp_quarters[house.0 as usize], 0);
    hills(&mut w, true);
    let before = w.building(house).unwrap().hp;
    hit(&mut w, a, house, 4, 1, DamageKind::Melee);
    assert_eq!(before - w.building(house).unwrap().hp, fresh_loss);
}
