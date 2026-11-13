use super::*;

#[test]
fn movement_ground_reuses_tiles_across_kinds_and_ticks_without_terrain_reads() {
    use std::cell::Cell;
    let mut world = GameWorld::new(WorldConfig::new(64, 64, Seed(0)).unwrap()).unwrap();
    world.terrain = flat_map_terrain();
    let tile = TileCoord::new(20, 20);
    assert!(
        world
            .unit_tile_passable(crate::UnitKind::Villager, tile, &|| false)
            .unwrap()
    );
    for kind in [crate::UnitKind::Villager, crate::UnitKind::Militia] {
        let before = world.canonical_hash();
        let checks = Cell::new(0);
        assert!(
            world
                .unit_tile_passable(kind, tile, &|| {
                    checks.set(checks.get() + 1);
                    false
                })
                .unwrap()
        );
        assert_eq!(
            checks.get(),
            1,
            "warm movement ground must only check cancellation"
        );
        assert_eq!(
            world.canonical_hash(),
            before,
            "movement ground is derived state"
        );
        assert_eq!(
            world.unit_tile_passable(kind, tile, &|| true),
            Err(EnvironmentPageError::Cancelled)
        );
        world.tick.0 += 1;
    }
}

#[test]
fn movement_ground_directed_edges_observe_cross_chunk_corner_changes() {
    let mut world = GameWorld::new(WorldConfig::new(64, 64, Seed(0)).unwrap()).unwrap();
    world.terrain = flat_map_terrain();
    let from = TileCoord::new(31, 31);
    let corner = TileCoord::new(32, 31);
    let to = TileCoord::new(32, 32);
    let area = TileRect::from_xywh(corner.x, corner.y, 1, 1);
    for blocked in [false, true, false] {
        let Terrain::Map { overlay, .. } = &mut world.terrain else {
            unreachable!()
        };
        if blocked {
            overlay.add_footprint(area).unwrap();
            world.collision_closed(area);
        } else {
            overlay.remove_footprint(area);
            world.collision_opened(area);
        }
        for (a, b) in [(from, to), (to, from), (from, corner), (corner, from)] {
            let expected = world.terrain.passable(b, world.config)
                && world.terrain.crossable(a, b, world.config);
            assert_eq!(
                world
                    .unit_step_clear(crate::UnitKind::Villager, a, b)
                    .unwrap(),
                expected,
                "blocked={blocked}, {a:?} -> {b:?}"
            );
            assert_eq!(
                world.economy.movement_ground[0]
                    .lock()
                    .unwrap()
                    .get_edge(a, b),
                Some(expected),
                "directed edges must retain an answer for subsequent movers"
            );
        }
        assert_eq!(
            world
                .unit_step_clear(crate::UnitKind::Villager, from, to)
                .unwrap(),
            !blocked
        );
        assert!(
            world
                .unit_step_clear(crate::UnitKind::Villager, corner, from)
                .unwrap(),
            "a blocked source must not reverse the directed destination rule"
        );
    }
}

#[test]
fn movement_ground_does_not_cache_failed_environment_reads() {
    let (package, pages, _) = economy::fixture::flat_fixture(false);
    let fail = Arc::new(AtomicBool::new(true));
    let provider = Arc::new(ToggleProvider {
        fail: fail.clone(),
        page: Arc::new(EnvironmentPage::Elevation(pages[0].clone())),
    });
    let mut world = GameWorld::new(WorldConfig::new(64, 64, Seed(0)).unwrap()).unwrap();
    world.terrain = Terrain::from_page_provider(&package, provider).unwrap();
    let tile = TileCoord::new(20, 20);
    let next = TileCoord::new(21, 20);
    assert_eq!(
        world.unit_tile_passable(crate::UnitKind::Villager, tile, &|| false),
        Err(EnvironmentPageError::Unavailable)
    );
    assert_eq!(
        world.unit_step_clear(crate::UnitKind::Villager, tile, next),
        Err(EnvironmentPageError::Unavailable)
    );
    fail.store(false, Ordering::Relaxed);
    assert!(
        world
            .unit_tile_passable(crate::UnitKind::Villager, tile, &|| false)
            .is_ok()
    );
    assert!(
        world
            .unit_step_clear(crate::UnitKind::Villager, tile, next)
            .is_ok()
    );
}

#[test]
fn movement_ground_observes_resource_depletion_and_snapshot_restore() {
    let (package, pages, water) = economy::fixture::flat_fixture(false);
    let hash = package.content_hash;
    let mut world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    let Terrain::Map { generator, .. } = &world.terrain else {
        unreachable!()
    };
    let node = (0..8)
        .flat_map(|x| generator.chunk(x, 0).unwrap().resources)
        .find(|node| ResourceOverlay::empty().blocks_node(*node))
        .expect("prepared land has a blocking resource");
    let snapshot = world.resource_snapshot(hash).unwrap();
    let before = world.canonical_hash();
    assert!(
        !world
            .unit_tile_passable(crate::UnitKind::Villager, node.tile, &|| false)
            .unwrap()
    );
    assert!(
        world
            .prepare_resource_depletion(node.id, node.initial_amount)
            .unwrap()
            .commit()
            .became_nonblocking
    );
    assert!(
        world
            .unit_tile_passable(crate::UnitKind::Villager, node.tile, &|| false)
            .unwrap()
    );
    world.restore_resources(&snapshot, hash, &|| false).unwrap();
    assert!(
        !world
            .unit_tile_passable(crate::UnitKind::Villager, node.tile, &|| false)
            .unwrap()
    );
    assert_eq!(world.canonical_hash(), before);
}
