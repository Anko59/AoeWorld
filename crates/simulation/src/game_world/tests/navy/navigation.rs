use super::*;
use crate::game_world::economy::NavalNavigation;
use aoe_map::{EnvironmentPageError, ResourceOverlay, RouteTerrain};
use std::cell::Cell;

fn query(world: &GameWorld, tile: TileCoord) -> bool {
    NavalNavigation::new(world)
        .route_walkable(ResourceOverlay::empty(), tile, &|| false)
        .unwrap()
}

#[test]
fn separate_ship_searches_reuse_ground_until_a_local_change() {
    let mut world = coast();
    let (_, water) = bank(&world);
    assert!(query(&world, water));
    let misses = world.economy.naval_ground.lock().unwrap().miss_count();
    let before = world.canonical_hash();
    let checks = Cell::new(0);
    assert!(
        NavalNavigation::new(&world)
            .route_walkable(ResourceOverlay::empty(), water, &|| {
                checks.set(checks.get() + 1);
                false
            })
            .unwrap()
    );
    assert_eq!(
        checks.get(),
        1,
        "a warm ground answer only checks cancellation"
    );
    assert_eq!(
        world.economy.naval_ground.lock().unwrap().miss_count(),
        misses
    );
    assert_eq!(world.canonical_hash(), before, "cache is derived state");
    let area = TileRect::from_xywh(water.x, water.y, 1, 1);
    world.economy.blocked_tiles.insert(water);
    world.collision_closed(area);
    assert!(
        !query(&world, water),
        "placing a building blocks a cached tile"
    );
    world.economy.blocked_tiles.remove(&water);
    world.collision_opened(area);
    assert!(query(&world, water), "removing it opens a cached tile");
    world.resource_collision_changed();
    assert!(query(&world, water));
}

#[test]
fn a_warm_ship_ground_answer_still_honours_cancellation() {
    let world = coast();
    let (_, water) = bank(&world);
    assert!(query(&world, water));
    assert_eq!(
        NavalNavigation::new(&world).route_walkable(ResourceOverlay::empty(), water, &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
    assert!(query(&world, water));
}

#[test]
fn cached_ship_edges_observe_obstacles_in_the_neighboring_chunk() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let from = water;
    let to = TileCoord::new(water.x + 1, water.y);
    let query_edge = |w: &GameWorld| {
        NavalNavigation::new(w)
            .route_edge_clear(from, to, &|| false)
            .unwrap()
    };
    assert!(query_edge(&world));
    assert_eq!(
        world
            .economy
            .naval_ground
            .lock()
            .unwrap()
            .get_edge(from, to),
        Some(true)
    );
    assert_eq!(
        NavalNavigation::new(&world).route_edge_clear(from, to, &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
    world.economy.blocked_tiles.insert(to);
    world.collision_closed(TileRect::from_xywh(to.x, to.y, 1, 1));
    assert!(!query_edge(&world));
    world.economy.blocked_tiles.remove(&to);
    world.collision_opened(TileRect::from_xywh(to.x, to.y, 1, 1));
    assert!(query_edge(&world));
}
