use super::super::VILLAGE_RULES;
use super::chunks::{BLOCK_TILES, HISTORY_VISITS};
use super::*;
use aoe_core::{Seed, TileRect, WorldConfig, WorldPosition};
use aoe_map::ResourceNode;

use crate::game_world::tests::economy::fixture;

mod facing;
mod occupancy;
mod rates;
mod replacement;

#[test]
fn returning_workers_deposit_on_every_side_before_finishing_an_old_target_order() {
    use super::super::BuildingKind;
    let template = fresh_task();
    for kind in [
        BuildingKind::TownCenter,
        BuildingKind::LumberCamp,
        BuildingKind::MiningCamp,
        BuildingKind::Mill,
        BuildingKind::Dock,
    ] {
        let resource = match kind {
            BuildingKind::LumberCamp => ResourceKind::Wood,
            BuildingKind::MiningCamp => ResourceKind::Gold,
            _ => ResourceKind::Food,
        };
        let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
        // The common return/deposit code must use each kind's full footprint.
        let rect = aoe_core::TileRect::from_xywh(128, 128, kind.rules().side, kind.rules().depth);
        let center = world
            .insert_building(PlayerId(0), kind, rect, true)
            .unwrap();
        for tile in perimeter(rect) {
            let worker = world
                .spawn_villager(PlayerId(0), position(tile).unwrap())
                .unwrap();
            let mut task = template.clone();
            task.kind = resource;
            // Even if the old trip targeted the opposite side, touching any
            // perimeter point must deposit immediately, without walking around.
            let target = if tile.y == rect.max.y {
                rect.min.y - 1
            } else {
                rect.max.y
            };
            let target = TileCoord::new(rect.max.x, target);
            world
                .issue_task_move(worker, position(target).unwrap())
                .unwrap();
            task.phase = GatherPhase::Returning;
            task.dropoff = Some(center);
            task.return_target = Some(target);
            let index = world.lookup[&worker];
            world.units[index].state.carried = Some(CarriedResource {
                kind: resource,
                amount: 10,
            });
            let before = world.player_economy(PlayerId(0)).unwrap().stockpile;
            world.economy.gatherers.insert(worker, task);
            world.advance_gathering(&mut |_| true, &mut Vec::new());
            assert!(
                world.unit(worker).unwrap().carried.is_none(),
                "{kind:?} at {tile:?}"
            );
            let mut expected = before;
            expected.deposit(CarriedResource {
                kind: resource,
                amount: 10,
            });
            assert_eq!(
                world.player_economy(PlayerId(0)).unwrap().stockpile,
                expected
            );
            assert!(world.economy.gatherers.get(&worker).is_none_or(|task| {
                task.phase == GatherPhase::Approaching && task.return_target.is_none()
            }));
            assert_ne!(
                world.movement_order(worker).map(|order| order.target_tile),
                Some(target),
                "Deposit must cancel the old return trip before resuming work"
            );
            world.remove_unit(worker);
        }
    }
}

#[test]
fn return_target_is_the_nearest_reachable_point_on_each_buildings_perimeter() {
    use super::super::BuildingKind;
    let template = fresh_task();
    for kind in [
        BuildingKind::TownCenter,
        BuildingKind::LumberCamp,
        BuildingKind::MiningCamp,
        BuildingKind::Mill,
        BuildingKind::Dock,
    ] {
        let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
        let rect = aoe_core::TileRect::from_xywh(128, 128, kind.rules().side, kind.rules().depth);
        let center = world
            .insert_building(PlayerId(0), kind, rect, true)
            .unwrap();
        for start in [
            TileCoord::new(rect.min.x - 8, rect.min.y),
            TileCoord::new(rect.min.x, rect.min.y - 8),
            TileCoord::new(rect.max.x + 8, rect.max.y - 1),
            TileCoord::new(rect.max.x - 1, rect.max.y + 8),
        ] {
            let worker = world
                .spawn_villager(PlayerId(0), position(start).unwrap())
                .unwrap();
            let mut task = template.clone();
            task.kind = match kind {
                BuildingKind::LumberCamp => ResourceKind::Wood,
                BuildingKind::MiningCamp => ResourceKind::Gold,
                _ => ResourceKind::Food,
            };
            // Shore fishermen can use a Dock too.
            task.fish = (kind == BuildingKind::Dock).then_some(aoe_map::ObjectKind::ShoreFish);
            assert!(world.begin_return(worker, &mut task));
            assert_eq!(task.dropoff, Some(center));
            let target = task.return_target.unwrap();
            assert_eq!(
                distance_squared(start, target),
                perimeter(rect)
                    .into_iter()
                    .map(|p| distance_squared(start, p))
                    .min()
                    .unwrap(),
                "{kind:?} from {start:?} chose {target:?}"
            );
            world.remove_unit(worker);
        }
    }
}

fn resource_mainland() -> (GameWorld, ResourceNode) {
    let (package, pages, water) = fixture::flat_fixture(false);
    let mut world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        unreachable!()
    };
    let mut nodes = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            nodes.extend(generator.chunk(x, y).unwrap().resources);
        }
    }
    let node = *nodes
        .iter()
        .find(|node| {
            node.kind == ResourceKind::Wood
                && (32..224).contains(&node.tile.x)
                && (32..224).contains(&node.tile.y)
        })
        .unwrap();
    for other in nodes {
        if other.id != node.id {
            overlay
                .deplete(generator, other.id, other.initial_amount)
                .unwrap();
        }
    }
    (world, node)
}

fn fresh_task() -> GatherTask {
    let (mut world, node) = resource_mainland();
    let start = TileCoord::new(node.tile.x - 1, node.tile.y);
    let villager = world
        .spawn_villager(PlayerId(0), position(start).unwrap())
        .unwrap();
    world.issue_gather(PlayerId(0), villager, node.id).unwrap();
    world.economy.gatherers.remove(&villager).unwrap()
}

fn return_world(start: TileCoord, isolated: TileCoord) -> (GameWorld, EntityId) {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    world
        .place_town_center(PlayerId(0), TileCoord::new(128, 128))
        .unwrap();
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx != 0 || dy != 0 {
                world
                    .economy
                    .blocked_tiles
                    .insert(TileCoord::new(isolated.x + dx, isolated.y + dy));
            }
        }
    }
    let villager = world
        .spawn_villager(PlayerId(0), position(start).unwrap())
        .unwrap();
    let index = world.lookup[&villager];
    world.units[index].state.carried = Some(CarriedResource {
        kind: ResourceKind::Wood,
        amount: 10,
    });
    let mut task = fresh_task();
    world.begin_return(villager, &mut task);
    // The closest perimeter tile is sealed off: the planner's sealed-
    // destination proof refuses it when the order is issued (it used to be a
    // search of the whole map), so the trip takes the next tile at once.
    assert_ne!(
        world.movement_order(villager).unwrap().target_tile,
        isolated
    );
    world.economy.gatherers.insert(villager, task);
    (world, villager)
}

fn assert_deposits_after_deferred_failure(start: TileCoord, isolated: TileCoord) {
    let (mut world, villager) = return_world(start, isolated);
    let mut left_initial_search = false;
    for _ in 0..2200 {
        world.advance();
        left_initial_search |= world.unit(villager).unwrap().moving;
        if world.player_economy(PlayerId(0)).unwrap().stockpile.wood == 10 {
            assert!(left_initial_search);
            assert!(world.unit(villager).unwrap().carried.is_none());
            return;
        }
    }
    panic!("villager kept retrying an unreachable perimeter instead of depositing");
}

#[test]
fn deferred_return_skips_the_isolated_closest_west_perimeter_tile() {
    assert_deposits_after_deferred_failure(TileCoord::new(88, 129), TileCoord::new(127, 129));
}

#[test]
fn deferred_return_skips_the_isolated_closest_corner_perimeter_tile() {
    assert_deposits_after_deferred_failure(TileCoord::new(97, 97), TileCoord::new(127, 127));
}

#[test]
fn exhausted_return_choices_stop_searching_and_keep_the_load() {
    let (mut world, villager) = return_world(TileCoord::new(88, 129), TileCoord::new(127, 129));
    for y in 126..=133 {
        for x in 126..=133 {
            if x == 126 || x == 133 || y == 126 || y == 133 {
                world
                    .insert_building(
                        PlayerId(1),
                        crate::BuildingKind::PalisadeWall,
                        TileRect::from_xywh(x, y, 1, 1),
                        true,
                    )
                    .unwrap();
            }
        }
    }
    for _ in 0..4000 {
        world.advance();
        if world.gather_target(villager).is_none() {
            assert!(
                world
                    .unit(villager)
                    .unwrap()
                    .carried
                    .is_some_and(|load| load.amount == 10)
            );
            assert!(world.movement_order(villager).is_none());
            let usage = world.navigation_cache.usage();
            for _ in 0..100 {
                world.advance();
            }
            assert_eq!(
                world.navigation_cache.usage(),
                usage,
                "terminal job must not restart searches"
            );
            return;
        }
    }
    panic!("all failed drop-off choices must leave a bounded terminal job");
}

#[test]
fn replacement_search_builds_each_touched_block_once_then_reuses_it() {
    let (mut world, node) = resource_mainland();
    let Terrain::Map { generator, overlay } = &mut world.terrain else {
        unreachable!()
    };
    // Grow starting history well past one replacement search. A history scan
    // per candidate tile would revisit every entry for each of the 317 tiles.
    let mut history = 0;
    'fill: for dy in -40..=40 {
        for dx in -40..=40 {
            for kind in [
                ResourceKind::Food,
                ResourceKind::Wood,
                ResourceKind::Gold,
                ResourceKind::Stone,
            ] {
                if history == 400 {
                    break 'fill;
                }
                let tile = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
                if let Some(start) = ResourceNode::starting(tile, kind)
                    && overlay.insert_starting_resource(generator, start).is_ok()
                {
                    history += 1;
                }
            }
        }
    }
    assert_eq!(history, 400, "fixture must place the full starting history");

    let radius = VILLAGE_RULES.replacement_radius_tiles;
    // The node itself is the exact match, so the search stops after the
    // block that holds it and reads only that block's history.
    let block = (
        node.tile.x.div_euclid(BLOCK_TILES),
        node.tile.y.div_euclid(BLOCK_TILES),
    );
    let span = TileCoord::new(block.0 * BLOCK_TILES, block.1 * BLOCK_TILES);
    let until = TileCoord::new(span.x + BLOCK_TILES - 1, span.y + BLOCK_TILES - 1);
    let within_touched_block = overlay.starting_nodes_in(span, until).count();
    HISTORY_VISITS.with(|visits| visits.set(0));
    let found = world.nearest_resource(node.tile, ResourceKind::Wood, radius, None);
    let cold = HISTORY_VISITS.with(|visits| visits.get());
    assert_eq!(found.map(|found| found.id), Some(node.id));
    assert_eq!(
        cold, within_touched_block,
        "a cold search reads the touched block's history once, not once per tile"
    );
    HISTORY_VISITS.with(|visits| visits.set(0));
    let again = world.nearest_resource(node.tile, ResourceKind::Wood, radius, None);
    let warm = HISTORY_VISITS.with(|visits| visits.get());
    assert_eq!(again.map(|found| found.id), Some(node.id));
    assert_eq!(warm, 0, "a warm search reuses the built block lists");
}

#[test]
fn deferred_unreachable_resource_ends_the_gather_job() {
    let (mut world, node) = resource_mainland();
    let Terrain::Map { overlay, .. } = &mut world.terrain else {
        unreachable!()
    };
    // Enclose 59 squared mainland cells around the island: more than one
    // 16,384-work poll, with no search of the unrelated rest of the map.
    // This outer ring plus the island ring uses exactly 256 obstacle tiles.
    for dy in -30..=30_i32 {
        for dx in -30..=30_i32 {
            if dx.abs() == 30 || dy.abs() == 30 {
                let tile = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
                overlay
                    .add_footprint(TileRect::new(tile, TileCoord::new(tile.x + 1, tile.y + 1)))
                    .unwrap();
            }
        }
    }
    for dy in -2..=2_i32 {
        for dx in -2..=2_i32 {
            if dx.abs() == 2 || dy.abs() == 2 {
                let tile = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
                overlay
                    .add_footprint(TileRect::new(tile, TileCoord::new(tile.x + 1, tile.y + 1)))
                    .unwrap();
            }
        }
    }
    let start = TileCoord::new(node.tile.x - 4, node.tile.y);
    let villager = world
        .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(start).unwrap())
        .unwrap();
    // Every tile around the node is sealed off: the sealed-destination proof
    // refuses the job when it is issued, or its search ends soon after.
    if world.issue_gather(PlayerId(0), villager, node.id).is_err() {
        assert!(world.gather_target(villager).is_none());
        assert!(world.movement_order(villager).is_none());
        return;
    }
    for _ in 0..256 {
        world.advance();
        if world.gather_target(villager).is_none() {
            assert_eq!(world.unit(villager).unwrap().task, UnitTask::Idle);
            assert!(world.movement_order(villager).is_none());
            assert_eq!(world.resource_remaining(node.id), Some(node.initial_amount));
            let usage = world.navigation_cache.usage();
            for _ in 0..100 {
                world.advance();
            }
            assert_eq!(world.navigation_cache.usage(), usage);
            return;
        }
    }
    panic!("an unreachable resource must stop instead of restarting its search");
}
