//! Firing positions must be reachable by the attacker, not by its target.
use super::*;
use crate::UnitKind;
use aoe_map::{FieldPyramid, MapPackage, PyramidLevel, WaterPage, ordered_water_page_root};

fn coast() -> (GameWorld, i32) {
    (water_world(false), 102)
}

fn water_world(island: bool) -> GameWorld {
    let (mut package, pages, _) = super::super::economy::fixture::flat_fixture(true);
    package.request.requested_side_meters = 15_360;
    let mut water = Vec::new();
    let mut levels = Vec::new();
    for level in 0..=8 {
        let axis = 256_u16 >> level;
        let mut level_pages = Vec::new();
        for py in 0..axis.div_ceil(64) {
            for px in 0..axis.div_ceil(64) {
                let side = axis.min(64) as u8;
                let ocean = (0..u16::from(side) * u16::from(side))
                    .map(|i| {
                        let x = (px * 64 + i % u16::from(side)) << level;
                        let y = (py * 64 + i / u16::from(side)) << level;
                        if x <= 102 || (island && (x, y) == (105, 100)) {
                            0
                        } else {
                            100
                        }
                    })
                    .collect();
                level_pages.push(WaterPage {
                    level,
                    x: px,
                    y: py,
                    width: side,
                    height: side,
                    ocean_coverage_percent: ocean,
                    inland_coverage_percent: vec![0; usize::from(side).pow(2)],
                });
            }
        }
        levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_water_page_root(&level_pages).unwrap(),
        });
        water.extend(level_pages);
    }
    package.environment.water = Some(FieldPyramid { levels });
    let package = MapPackage::with_prepared_environment(
        1,
        package.request,
        Vec::new(),
        package.projection,
        package.provenance,
        package.environment,
    )
    .unwrap();
    GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap()
}

#[test]
fn archer_walks_to_shore_and_shoots_a_ship_on_water() {
    let (mut w, shore) = coast();
    let archer = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(shore - 2, 100))
        .unwrap();
    let ship = w
        .spawn_kind(PlayerId(1), UnitKind::FishingShip, at(shore + 3, 100))
        .unwrap();
    w.issue_attack(PlayerId(0), archer, ship)
        .expect("shore firing position is reachable");
    assert_eq!(
        w.movement_order(archer).unwrap().destination,
        at(shore - 1, 100)
    );
    ticks_until(&mut w, 100, |w| w.unit(ship).unwrap().hp < 50);
    assert!(w.unit(archer).unwrap().position.x <= at(shore, 100).x);
    assert!(w.tile_passable(w.unit(archer).unwrap().position.tile_floor()));
}

fn island(kind: UnitKind) -> (GameWorld, EntityId, EntityId) {
    let mut w = water_world(true);
    let attacker = w.spawn_kind(PlayerId(0), kind, at(100, 100)).unwrap();
    let target = w.spawn_villager(PlayerId(1), at(105, 100)).unwrap();
    // A real land tile surrounded by eight water tiles.
    for y in 99..=101 {
        for x in 104..=106 {
            if (x, y) != (105, 100) {
                assert!(!w.tile_passable(TileCoord::new(x, y)));
            }
        }
    }
    (w, attacker, target)
}

#[test]
fn skirmisher_fires_at_an_isolated_target_without_pathing_onto_the_island() {
    let (mut w, skirm, target) = island(UnitKind::Skirmisher);
    w.issue_attack(PlayerId(0), skirm, target).unwrap();
    assert_eq!(w.movement_order(skirm).unwrap().destination, at(101, 100));
    ticks_until(&mut w, 100, |w| w.unit(target).unwrap().hp < 25);
    assert_eq!(w.unit(skirm).unwrap().attack_target, Some(target));
}

#[test]
fn attack_without_a_reachable_firing_position_is_rejected_clearly() {
    let (mut w, militia, target) = island(UnitKind::Militia);
    let error = w.issue_attack(PlayerId(0), militia, target).unwrap_err();
    assert!(
        error.to_string().contains("reachable attack position"),
        "{error}"
    );
    assert_eq!(w.unit(militia).unwrap().attack_target, None);
    assert!(w.movement_order(militia).is_none());
}

#[test]
fn melee_approaches_adjacent_to_an_impassable_target_tile() {
    let mut w = world();
    let militia = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let target = w.spawn_villager(PlayerId(1), at(103, 100)).unwrap();
    w.economy.blocked_tiles.insert(TileCoord::new(103, 100));
    w.issue_attack(PlayerId(0), militia, target).unwrap();
    assert_eq!(w.movement_order(militia).unwrap().destination, at(102, 100));
    ticks_until(&mut w, 100, |w| w.unit(target).unwrap().hp < 25);
}

#[test]
fn shore_attack_replay_preserves_the_canonical_hash() {
    let run = || {
        let (mut w, shore) = coast();
        let archer = w
            .spawn_kind(PlayerId(0), UnitKind::Archer, at(shore - 2, 100))
            .unwrap();
        let ship = w
            .spawn_kind(PlayerId(1), UnitKind::FishingShip, at(shore + 3, 100))
            .unwrap();
        w.issue_attack(PlayerId(0), archer, ship).unwrap();
        advance(&mut w, 100);
        w.canonical_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn an_unchanged_firing_destination_keeps_the_one_second_repath_cadence() {
    let mut w = world();
    let attacker = w.spawn_militia(PlayerId(0), at(100, 100)).unwrap();
    let target = w.spawn_villager(PlayerId(1), at(110, 100)).unwrap();
    w.issue_attack(PlayerId(0), attacker, target).unwrap();
    let first_due = w.combat.attackers[&attacker].repath_tick;
    advance(&mut w, 20);
    assert_eq!(
        w.movement_order(attacker).unwrap().destination,
        at(109, 100)
    );
    assert_eq!(w.combat.attackers[&attacker].repath_tick, first_due + 20);
}

#[test]
fn a_trapped_archer_rejects_an_attack_outside_its_range() {
    let mut w = world();
    let archer = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, at(100, 100))
        .unwrap();
    let target = w.spawn_villager(PlayerId(1), at(105, 100)).unwrap();
    for y in 99..=101 {
        for x in 99..=101 {
            if (x, y) != (100, 100) {
                w.economy.blocked_tiles.insert(TileCoord::new(x, y));
            }
        }
    }
    let before = w.canonical_hash();
    let error = w.issue_attack(PlayerId(0), archer, target).unwrap_err();
    assert!(
        error.to_string().contains("reachable attack position"),
        "{error}"
    );
    assert_eq!(
        w.canonical_hash(),
        before,
        "rejection leaves the order state intact"
    );
}
