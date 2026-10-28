//! Defensive buildings keep firing while a huge army spends the unit scan budget.
use super::*;
use crate::{BuildingKind, UnitKind};

/// A big two-player crowd packed into the south-west, so many seekers scan.
fn busy_world(units: u32) -> GameWorld {
    let config = WorldConfig::new(256, 256, Seed(11)).unwrap();
    GameWorld::with_population_in_extent(config, units, 0, 2, 64).unwrap()
}

fn garrisoned_town_center(world: &mut GameWorld, x: i32, y: i32) -> EntityId {
    let side = BuildingKind::TownCenter.rules().side;
    let tc = world
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            TileRect::from_xywh(x, y, side, side),
            true,
        )
        .unwrap();
    for _ in 0..5 {
        let v = world
            .spawn_kind(PlayerId(0), UnitKind::Villager, at(x - 1, y))
            .unwrap();
        world.issue_garrison(PlayerId(0), v, tc).unwrap();
    }
    advance(world, 5);
    assert!(world.building_arrow_count(tc) > 0);
    tc
}

fn volleys(world: &mut GameWorld, tc: EntityId, ticks: u32) -> (u32, u32) {
    let mut seen = std::collections::BTreeSet::new();
    let mut worst = 0;
    for _ in 0..ticks {
        world.advance();
        worst = worst.max(world.combat_stats().targeting_scanned);
        seen.extend(
            world
                .projectiles()
                .filter(|p| p.attacker == tc)
                .map(|p| p.id),
        );
    }
    (seen.len() as u32, worst)
}

/// Volleys launched by `tc` within `ticks`, with fog of war on.
fn fog_volleys(world: &mut GameWorld, tc: EntityId, ticks: u32) -> u32 {
    world.set_fog_disabled(false);
    volleys(world, tc, ticks).0
}

#[test]
fn a_garrisoned_town_center_shoots_one_visible_enemy() {
    for size in [256, 2000] {
        let config = WorldConfig::new(size, size, Seed(11)).unwrap();
        let mut world = GameWorld::new(config).unwrap();
        let (x, y) = (size - 100, size - 100);
        let tc = garrisoned_town_center(&mut world, x, y);
        world.spawn_militia(PlayerId(1), at(x + 5, y + 2)).unwrap();
        assert!(fog_volleys(&mut world, tc, 20) >= 5, "map {size}");
    }
}

#[test]
fn town_center_range_is_measured_from_its_edge() {
    // A 4x4 Town Center reaches 6 tiles past its edge (over 6 from its centre).
    for (dx, fires) in [(7, true), (9, false)] {
        let (mut world, tc) = {
            let mut world = busy_world(0);
            let tc = garrisoned_town_center(&mut world, 100, 100);
            (world, tc)
        };
        world.spawn_militia(PlayerId(1), at(102 + dx, 102)).unwrap();
        world
            .issue_stance(
                PlayerId(1),
                &[world.units().last().unwrap().id],
                aoe_core::CombatStance::NoAttack,
            )
            .unwrap();
        assert_eq!(fog_volleys(&mut world, tc, 20) > 0, fires, "dx {dx}");
    }
}

#[test]
fn a_hundred_garrisoned_town_centers_fire_while_an_army_spends_the_scan_budget() {
    let mut world = busy_world(11_000);
    let spot = |gx: i32, gy: i32| (2 + gx * 6, 2 + gy * 6);
    let tcs: Vec<_> = (0..100)
        .map(|n| {
            let (x, y) = spot(n % 10, n / 10);
            garrisoned_town_center(&mut world, x, y)
        })
        .collect();
    for n in 0..100 {
        let (x, y) = spot(n % 10, n / 10);
        world.spawn_militia(PlayerId(1), at(x + 5, y + 5)).unwrap();
    }
    let mut fired = std::collections::BTreeSet::new();
    for _ in 0..150 {
        world.advance();
        assert!(world.combat_stats().targeting_scanned <= 16_384);
        fired.extend(
            world
                .projectiles()
                .map(|p| p.attacker)
                .filter(|id| tcs.contains(id)),
        );
    }
    assert_eq!(fired.len(), tcs.len(), "every Town Center fired within 5 s");
}
