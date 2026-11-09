//! Transient terrain failures must never become permanent region closures.
use super::*;

#[test]
fn a_failed_region_build_is_retried_after_the_provider_recovers() {
    let (terrain, fail) = toggle_provider_terrain();
    let mut world = GameWorld::new(WorldConfig::new(64, 64, Seed(0)).unwrap()).unwrap();
    world.terrain = terrain;
    let from = TileCoord::new(1, 1);
    let to = TileCoord::new(60, 1);
    fail.store(true, Ordering::Relaxed);
    assert_eq!(
        world.land_reachable(PlayerId(0), aoe_map::LandBucket::Small, from, to),
        Err(GameWorldError::Environment(
            EnvironmentPageError::Unavailable
        ))
    );
    fail.store(false, Ordering::Relaxed);
    assert!(
        world
            .land_reachable(PlayerId(0), aoe_map::LandBucket::Small, from, to)
            .unwrap()
    );
}

#[test]
fn sealed_near_searches_are_answered_by_regions_even_with_one_work_per_tick() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    for n in 88..=168 {
        for tile in [
            TileCoord::new(n, 88),
            TileCoord::new(n, 168),
            TileCoord::new(88, n),
            TileCoord::new(168, n),
        ] {
            world.economy.blocked_tiles.insert(tile);
        }
    }
    world.fast_route_budget = 0;
    let target = WorldPosition::from_tile_center(TileCoord::new(128, 128)).unwrap();
    let ids: Vec<_> = (0..6)
        .map(|n| {
            let id = world
                .spawn_unit(
                    PlayerId(0),
                    WorldPosition::from_tile_center(TileCoord::new(84, 124 + n)).unwrap(),
                )
                .unwrap();
            assert!(world.issue_task_move(id, target).unwrap());
            id
        })
        .collect();
    for _ in 0..10 {
        world.planning_budget = 1;
        world.fast_route_budget = 0;
        world.advance();
    }
    for id in ids {
        assert!(
            world.movement_order(id).is_none(),
            "{id:?} still searches sealed ground"
        );
        assert_eq!(
            world.movement_failure(id),
            Some(GameWorldError::Unreachable)
        );
    }
}
