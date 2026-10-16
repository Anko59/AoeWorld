use super::*;
use aoe_core::{Seed, TileRect, WorldConfig, WorldPosition};

use crate::game_world::tests::economy::fixture;

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
    assert!(
        world.unit(villager).unwrap().planning,
        "initial search must defer beyond 16384 work"
    );
    assert_eq!(
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
                world.economy.blocked_tiles.insert(TileCoord::new(x, y));
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
fn replacement_search_checks_starting_history_once_per_search() {
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
    let candidates = (-radius..=radius)
        .flat_map(|dy| (-radius..=radius).map(move |dx| (dx, dy)))
        .filter(|(dx, dy)| dx * dx + dy * dy <= radius * radius)
        .count();
    HISTORY_VISITS.with(|visits| visits.set(0));
    let found = world.nearest_resource(node.tile, ResourceKind::Wood, radius, None);
    let visits = HISTORY_VISITS.with(|visits| visits.get());
    assert_eq!(found.map(|found| found.id), Some(node.id));
    assert!(
        visits <= candidates,
        "one replacement search touched {visits} history entries for {candidates} candidate tiles"
    );
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
    world.issue_gather(PlayerId(0), villager, node.id).unwrap();
    assert!(
        world.unit(villager).unwrap().planning,
        "mainland search must exceed the initial budget"
    );
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
