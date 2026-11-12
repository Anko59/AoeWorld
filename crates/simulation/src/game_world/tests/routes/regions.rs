//! Transient terrain failures must never become permanent region closures.
use super::*;
use crate::MAX_ROUTE_WORK_PER_ORDER;

mod guidance;

#[test]
fn a_certified_frontier_route_survives_the_disconnected_requested_goal() {
    let mut world = GameWorld::new(WorldConfig::new(64, 64, Seed(0)).unwrap()).unwrap();
    world.terrain = flat_map_terrain();
    let Terrain::Map { overlay, .. } = &mut world.terrain else {
        unreachable!();
    };
    for rect in [
        TileRect::from_xywh(25, 25, 5, 1),
        TileRect::from_xywh(25, 29, 5, 1),
        TileRect::from_xywh(25, 26, 1, 3),
        TileRect::from_xywh(29, 26, 1, 3),
    ] {
        overlay.add_footprint(rect).unwrap();
    }
    let from = TileCoord::new(20, 27);
    let target = TileCoord::new(27, 27);
    let id = world
        .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(from).unwrap())
        .unwrap();
    world.fast_route_budget = 0;
    world
        .issue_task_move(id, WorldPosition::from_tile_center(target).unwrap())
        .unwrap();
    let index = world.lookup[&id];
    let mut planner = world.take_planner(index).unwrap();
    // Stop at the first certified endpoint, before its path has been emitted.
    for _ in 0..MAX_ROUTE_WORK_PER_ORDER {
        assert_eq!(
            world.poll_world_route_planner(
                &mut planner,
                1,
                &|| false,
                UnitKind::Villager,
                aoe_core::PlayerId(0)
            ),
            RoutePlannerPoll::Pending
        );
        if planner.certified_destination().is_some() {
            break;
        }
    }
    let certified = planner.certified_destination().unwrap();
    assert_ne!(certified, target);
    assert!(
        !world
            .land_reachable(PlayerId(0), aoe_map::LandBucket::Small, from, target)
            .unwrap()
    );
    assert!(
        world
            .land_reachable(PlayerId(0), aoe_map::LandBucket::Small, from, certified)
            .unwrap()
    );
    world.store_planner(index, Some(planner));
    let mut changed = Vec::new();
    world.serve_route_searches(0, &mut changed);
    assert!(
        world.movement_order(id).is_some(),
        "the certified route was discarded"
    );
    assert_eq!(world.movement_failure(id), None);
    assert!(world.active_movers.contains(id));
    assert_eq!(world.route_accounting_error(), None);

    // A certificate is not an exemption from current connectivity: closing
    // the replacement endpoint must still end the waiting route.
    let area = TileRect::from_xywh(certified.x, certified.y, 1, 1);
    let Terrain::Map { overlay, .. } = &mut world.terrain else {
        unreachable!();
    };
    overlay.add_footprint(area).unwrap();
    world.regions_changed(area);
    world.serve_route_searches(0, &mut changed);
    assert!(world.movement_order(id).is_none());
    assert_eq!(
        world.movement_failure(id),
        Some(GameWorldError::Unreachable)
    );
    assert!(!world.active_movers.contains(id));
    assert_eq!(world.route_accounting_error(), None);
}

#[test]
fn a_failed_region_build_is_retried_after_the_provider_recovers() {
    let (mut terrain, fail) = toggle_provider_terrain();
    // Node depletion projects every tile during setup. A provider failure
    // needs a genuine cache miss, rather than unrelated chunk reads.
    if let Terrain::Map { generator, .. } = &mut terrain {
        *generator = generator.navigation_preparation_worker();
    }
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
fn a_cached_region_witness_does_not_require_unrelated_source_reads() {
    let (terrain, fail) = toggle_provider_terrain();
    let mut world = GameWorld::new(WorldConfig::new(64, 64, Seed(0)).unwrap()).unwrap();
    world.terrain = terrain;
    let from = TileCoord::new(1, 1);
    let to = TileCoord::new(60, 1);
    assert_eq!(
        world.land_reachable(PlayerId(0), aoe_map::LandBucket::Small, from, to),
        Ok(true)
    );
    fail.store(true, Ordering::Relaxed);
    assert_eq!(
        world.land_reachable(PlayerId(0), aoe_map::LandBucket::Small, from, to),
        Ok(true),
        "a cached immutable witness is still valid while its provider is unavailable"
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

#[test]
fn confirmed_regions_do_not_repeat_a_destination_pocket_flood() {
    use aoe_map::{NavigationPhase, NavigationProbe, NavigationSpan};
    use std::sync::atomic::AtomicUsize;
    #[derive(Debug, Default)]
    struct Probe(AtomicUsize);
    struct Span;
    impl NavigationSpan for Span {}
    impl NavigationProbe for Probe {
        fn begin(&self, phase: NavigationPhase) -> Box<dyn NavigationSpan> {
            if phase == NavigationPhase::Pocket {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
            Box::new(Span)
        }
    }
    let (terrain, _) = toggle_provider_terrain();
    let mut world = GameWorld::new(WorldConfig::new(64, 64, Seed(0)).unwrap()).unwrap();
    world.terrain = terrain;
    let wall = TileRect::from_xywh(32, 0, 1, 56);
    let Terrain::Map { overlay, generator } = &mut world.terrain else {
        unreachable!();
    };
    overlay.add_footprint(wall).unwrap();
    let probe = Arc::new(Probe::default());
    generator.set_navigation_probe(probe.clone());
    world.regions_changed(wall);
    let from = TileCoord::new(10, 10);
    let target = TileCoord::new(50, 10);
    assert!(
        world
            .land_reachable(PlayerId(0), aoe_map::LandBucket::Small, from, target)
            .unwrap()
    );
    let id = world
        .spawn_unit(PlayerId(0), WorldPosition::from_tile_center(from).unwrap())
        .unwrap();
    world.fast_route_budget = 0;
    assert!(
        world
            .issue_task_move(id, WorldPosition::from_tile_center(target).unwrap())
            .unwrap()
    );
    let mut changed = Vec::new();
    for _ in 0..16 {
        world.serve_route_searches(1024, &mut changed);
    }
    assert_eq!(world.movement_failure(id), None);
    assert!(world.movement_order(id).is_some());
    assert_eq!(world.route_accounting_error(), None);
    assert_eq!(
        probe.0.load(Ordering::Relaxed),
        0,
        "current region connectivity already rules out a sealed destination pocket"
    );
}

#[test]
fn far_search_reuses_region_connectivity_before_flooding_the_destination() {
    use aoe_map::{NavigationPhase, NavigationProbe, NavigationSpan};
    use std::sync::atomic::AtomicUsize;
    #[derive(Debug, Default)]
    struct Probe(AtomicUsize);
    struct Span;
    impl NavigationSpan for Span {}
    impl NavigationProbe for Probe {
        fn begin(&self, phase: NavigationPhase) -> Box<dyn NavigationSpan> {
            if phase == NavigationPhase::Pocket {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
            Box::new(Span)
        }
    }
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    world.terrain = flat_map_terrain_size(256);
    let wall = TileRect::from_xywh(128, 0, 1, 240);
    let Terrain::Map { overlay, generator } = &mut world.terrain else {
        unreachable!();
    };
    overlay.add_footprint(wall).unwrap();
    let probe = Arc::new(Probe::default());
    generator.set_navigation_probe(probe.clone());
    world.regions_changed(wall);
    let from = TileCoord::new(120, 10);
    let target = TileCoord::new(220, 10);
    assert!(
        world
            .land_reachable(PlayerId(0), aoe_map::LandBucket::Small, from, target)
            .unwrap()
    );
    let id = world
        .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(from).unwrap())
        .unwrap();
    world.fast_route_budget = 0;
    assert!(
        world
            .issue_task_move(id, WorldPosition::from_tile_center(target).unwrap())
            .unwrap()
    );
    for _ in 0..16 {
        world.serve_route_searches(1024, &mut Vec::new());
    }
    assert_eq!(world.movement_failure(id), None);
    assert!(world.movement_order(id).is_some());
    assert_eq!(world.route_accounting_error(), None);
    assert_eq!(
        probe.0.load(Ordering::Relaxed),
        0,
        "a reachable far endpoint needs no second destination-pocket flood"
    );
}
