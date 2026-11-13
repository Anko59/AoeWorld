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
    let misses = world.economy.movement_ground[1]
        .lock()
        .unwrap()
        .miss_count();
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
        world.economy.movement_ground[1]
            .lock()
            .unwrap()
            .miss_count(),
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
        world.economy.movement_ground[1]
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

#[test]
fn movement_ground_keeps_land_and_water_separate_when_bridges_change() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let span = aoe_core::BridgeSpan::new(water, TileCoord::new(water.x + 4, water.y)).unwrap();
    let tile = TileCoord::new(water.x + 2, water.y);
    let next = TileCoord::new(tile.x + 1, tile.y);
    for bridged in [false, true, false] {
        let crate::Terrain::Map { overlay, .. } = &mut world.terrain else {
            unreachable!()
        };
        if bridged {
            overlay.add_bridge(span).unwrap();
            world.collision_opened(span.footprint());
        } else {
            overlay.remove_bridge(span);
            world.collision_closed(span.footprint());
        }
        assert_eq!(
            world
                .unit_tile_passable(UnitKind::Villager, tile, &|| false)
                .unwrap(),
            bridged
        );
        assert_eq!(
            world
                .unit_tile_passable(UnitKind::FishingShip, tile, &|| false)
                .unwrap(),
            !bridged
        );
        assert_eq!(
            world
                .unit_step_clear(UnitKind::Villager, tile, next)
                .unwrap(),
            bridged
        );
        assert_eq!(
            world
                .unit_step_clear(UnitKind::FishingShip, tile, next)
                .unwrap(),
            !bridged
        );
    }
}

#[test]
fn movement_and_planning_share_one_ground_answer_for_all_ship_kinds() {
    let mut world = coast();
    let (_, water) = bank(&world);
    world.economy.movement_ground[1].get_mut().unwrap().clear();
    let misses = world.economy.movement_ground[1]
        .lock()
        .unwrap()
        .miss_count();
    let before = world.canonical_hash();
    assert!(
        world
            .unit_tile_passable(UnitKind::Galley, water, &|| false)
            .unwrap()
    );
    assert_eq!(
        world.economy.movement_ground[1]
            .lock()
            .unwrap()
            .miss_count(),
        misses + 1
    );
    for _ in 0..3 {
        for kind in [
            UnitKind::FishingShip,
            UnitKind::Galley,
            UnitKind::TransportShip,
        ] {
            assert!(world.unit_tile_passable(kind, water, &|| false).unwrap());
            assert_eq!(
                world.unit_tile_passable(kind, water, &|| true),
                Err(EnvironmentPageError::Cancelled)
            );
        }
        assert!(query(&world, water));
    }
    assert_eq!(
        world.economy.movement_ground[1]
            .lock()
            .unwrap()
            .miss_count(),
        misses + 1
    );
    assert_eq!(world.canonical_hash(), before);
}

#[test]
fn shared_movement_ground_observes_footprints_and_bridge_edits() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let area = TileRect::from_xywh(water.x, water.y, 1, 1);
    let wet = |world: &GameWorld| {
        world
            .unit_tile_passable(UnitKind::Galley, water, &|| false)
            .unwrap()
    };
    assert!(wet(&world));
    let Terrain::Map { overlay, .. } = &mut world.terrain else {
        panic!("map");
    };
    overlay.add_footprint(area).unwrap();
    world.collision_closed(area);
    assert!(!wet(&world));
    let Terrain::Map { overlay, .. } = &mut world.terrain else {
        panic!("map");
    };
    overlay.remove_footprint(area);
    world.collision_opened(area);
    assert!(wet(&world));
    let span = aoe_core::BridgeSpan::new(
        TileCoord::new(water.x - 1, water.y),
        TileCoord::new(water.x + 1, water.y),
    )
    .unwrap();
    let Terrain::Map { overlay, .. } = &mut world.terrain else {
        panic!("map");
    };
    overlay.add_bridge(span).unwrap();
    world.collision_closed(span.footprint());
    assert!(!wet(&world));
    let Terrain::Map { overlay, .. } = &mut world.terrain else {
        panic!("map");
    };
    overlay.remove_bridge(span);
    world.collision_opened(span.footprint());
    assert!(wet(&world));
}

#[test]
fn planner_reservations_do_not_poison_shared_physical_ground() {
    let mut world = coast();
    let (_, water) = bank(&world);
    world.economy.movement_ground[1].get_mut().unwrap().clear();
    world.economy.blocked_tiles.insert(water);
    assert!(!query(&world, water));
    assert!(
        world
            .unit_tile_passable(UnitKind::Galley, water, &|| false)
            .unwrap()
    );
    assert_eq!(
        world.economy.movement_ground[1].lock().unwrap().get(water),
        Some(true)
    );
    assert!(!query(&world, water));
    world.economy.blocked_tiles.remove(&water);
    assert!(query(&world, water));
}
