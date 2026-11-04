//! Preconditions for the recorded kingdom shortage showcase.
use crate::{BuildingKind, GameWorld, Stockpile, UnitKind, UnitTask};
use aoe_core::{PlayerId, Seed, TileCoord};
use aoe_map::ResourceKind;
use std::collections::BTreeSet;

#[test]
fn stability_showcase_preserves_working_farms_after_the_owner_activates_management() {
    let (mut w, _) = GameWorld::steward_stability_demo(Seed(7)).unwrap();
    let king = w.units().find(|u| u.kind == UnitKind::Steward).unwrap().id;
    let town_center = w
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .id;
    let workers: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    assert_eq!(workers.len(), 20);
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().population_used as usize,
        w.units().count(),
        "capture setup must keep the living population exact"
    );
    assert_eq!(w.steward_status(king), None);
    assert!(
        w.movement_order(king).is_none(),
        "the recording starts before activation"
    );
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().stockpile,
        Stockpile {
            food: 1_800,
            wood: 1_000,
            gold: 30_000,
            stone: 30_000
        }
    );
    assert!(
        workers
            .iter()
            .all(|id| w.unit(*id).unwrap().task == UnitTask::Farming
                && w.steward_controller(*id).is_none())
    );
    w.issue_garrison(PlayerId(0), king, town_center).unwrap();
    for _ in 0..400 {
        w.advance();
        if w.steward_work().domains == 1 && w.steward_status(king).is_some_and(|s| s.managed == 20)
        {
            break;
        }
    }
    assert!(
        workers
            .iter()
            .all(|id| w.steward_controller(*id) == Some(king))
    );
    let plots: Vec<_> = workers.iter().map(|id| (*id, w.gather_tile(*id))).collect();
    let food = w.player_economy(PlayerId(0)).unwrap().stockpile.food;
    for _ in 0..200 {
        w.advance();
        for (worker, plot) in &plots {
            assert_eq!(w.gathering_kind(*worker), Some(ResourceKind::Food));
            assert_eq!(
                w.gather_tile(*worker),
                *plot,
                "management preserves productive plots"
            );
        }
    }
    assert!(
        w.player_economy(PlayerId(0)).unwrap().stockpile.food > food,
        "the retained farmers still deliver real Food"
    );
}

#[test]
fn kingdom_showcase_starts_with_twenty_distinct_working_farms_before_management() {
    let (w, primary) = GameWorld::steward_kingdom_demo(Seed(7)).unwrap();
    assert_eq!(w.config().tick_hz, 5);
    assert_eq!(w.unit(primary).unwrap().kind, UnitKind::Scout);
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().stockpile,
        Stockpile {
            food: 30_000,
            wood: 1_000,
            gold: 30_000,
            stone: 30_000
        }
    );
    assert_eq!(
        w.buildings()
            .filter(|b| b.kind == BuildingKind::TownCenter)
            .flat_map(|b| &b.training_queue)
            .filter(|entry| entry.kind == UnitKind::Villager)
            .count(),
        0
    );
    let workers: Vec<_> = w.units().filter(|u| u.kind == UnitKind::Villager).collect();
    assert_eq!(workers.len(), 20);
    let mut worked = BTreeSet::new();
    for worker in workers {
        assert_eq!(worker.player, PlayerId(0));
        assert_eq!(
            worker.task,
            UnitTask::Farming,
            "worker {:?} is already productive",
            worker.id
        );
        assert_eq!(
            w.steward_controller(worker.id),
            None,
            "initial jobs precede management"
        );
        assert_eq!(w.gathering_kind(worker.id), Some(ResourceKind::Food));
        let target = w.gather_tile(worker.id).expect("ordinary Farm job");
        let farm = w
            .buildings()
            .find(|b| b.kind == BuildingKind::Farm && b.footprint.contains(target))
            .unwrap();
        assert!(farm.is_complete());
        assert!(farm.food_remaining.is_some_and(|food| food > 0));
        assert!(farm.footprint.contains(worker.position.tile_floor()));
        assert!(worked.insert(farm.id), "each worker has its own crop");
    }
    assert_eq!(
        w.buildings()
            .filter(|b| b.kind == BuildingKind::Farm)
            .count(),
        20
    );
    let stewards: Vec<_> = w.units().filter(|u| u.kind == UnitKind::Steward).collect();
    assert_eq!(stewards.len(), 1);
    assert!(
        w.steward_status(stewards[0].id).is_none(),
        "recording starts before Town Center entry"
    );
}

#[test]
fn kingdom_showcase_has_legal_nonoverlapping_buildings_and_two_preserved_wood_sources() {
    let (mut w, _) = GameWorld::steward_kingdom_demo(Seed(7)).unwrap();
    let sites: Vec<_> = w.buildings().map(|b| (b.id, b.kind, b.footprint)).collect();
    assert!(
        sites
            .iter()
            .all(|(_, _, footprint)| footprint
                .valid(w.config().width_tiles, w.config().height_tiles))
    );
    for (at, (_, _, first)) in sites.iter().enumerate() {
        for (_, _, second) in &sites[at + 1..] {
            assert!(
                !(first.min.x < second.max.x
                    && second.min.x < first.max.x
                    && first.min.y < second.max.y
                    && second.min.y < first.max.y),
                "overlapping showcase sites: {first:?}, {second:?}"
            );
        }
    }
    let camp = w
        .buildings()
        .find(|b| b.kind == BuildingKind::LumberCamp)
        .unwrap();
    let from = TileCoord::new(camp.footprint.min.x + 1, camp.footprint.min.y + 1);
    let first = w
        .nearest_resource(from, ResourceKind::Wood, 10, None)
        .expect("nearby preserved tree");
    let second = w
        .nearest_resource(first.tile, ResourceKind::Wood, 10, Some(first.id))
        .expect("second preserved tree");
    assert_ne!(first.id, second.id);
    for tree in [first, second] {
        assert!(w.resource_remaining(tree.id).unwrap() > 0);
        assert!(
            sites
                .iter()
                .all(|(_, _, footprint)| !footprint.contains(tree.tile))
        );
    }
    // Remove occupants and their own collision before using the normal validator.
    // Pairwise overlap is checked above, so removal cannot conceal overlapping sites.
    let units: Vec<_> = w.units().map(|u| u.id).collect();
    for id in units {
        w.remove_unit(id);
    }
    for (id, _, _) in &sites {
        w.remove_building(*id);
    }
    for (_, kind, footprint) in sites {
        w.validate_building_placement(kind, footprint.min)
            .unwrap_or_else(|error| {
                panic!("illegal showcase {kind:?} at {footprint:?}: {error:?}")
            });
    }
    assert!(w.resource_remaining(first.id).unwrap() > 0);
    assert!(w.resource_remaining(second.id).unwrap() > 0);
}
#[test]
fn throughput_showcase_starts_forty_idle_workers_then_orders_all_at_normal_game_speed() {
    let (mut w, _) = GameWorld::steward_throughput_demo(aoe_core::Seed(7)).unwrap();
    assert_eq!(w.config().tick_hz, 20);
    let king = w.units().find(|u| u.kind == UnitKind::Steward).unwrap().id;
    let town_center = w
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .id;
    let workers: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    assert_eq!(workers.len(), 40);
    assert!(
        workers
            .iter()
            .all(|id| w.unit(*id).unwrap().task == UnitTask::Idle)
    );
    assert_eq!(w.steward_status(king), None);
    w.issue_garrison(PlayerId(0), king, town_center).unwrap();
    for _ in 0..2 * w.config().tick_hz {
        w.advance();
    }
    assert_eq!(
        workers
            .iter()
            .filter(|id| w.steward_controller(**id) == Some(king)
                && w.gathering_kind(**id) == Some(ResourceKind::Wood))
            .count(),
        40,
        "{}",
        super::throughput::missing_work(&w, king, &workers)
    );
}

#[test]
fn kingdom_demo_uses_the_large_world_extent_and_replays_deterministically() {
    let (mut a, _) = GameWorld::steward_kingdom_demo(Seed(7)).unwrap();
    assert_eq!(
        [a.config().width_tiles, a.config().height_tiles],
        [8000, 8000]
    );
    let (mut b, _) = GameWorld::steward_kingdom_demo(Seed(7)).unwrap();
    for _ in 0..64 {
        a.advance();
        b.advance();
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }
}
