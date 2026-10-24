//! Startup must not compose the map or flood its chunks.
use super::super::*;
use crate::navigation::long_routes_tests::flat_terrain_with_water_wall;
use std::time::Instant;

#[test]
fn france_sized_preparation_does_not_visit_terrain() {
    chunk_preparation_cost_breakdown();
    let generator = flat_terrain_with_water_wall(16_000, -1, -1);
    let overlay = ResourceOverlay::default();
    let before = generator.walk_cache_usage();
    let probes = std::cell::Cell::new(0);
    generator
        .prepare_navigation(&overlay, &|| {
            probes.set(probes.get() + 1);
            probes.get() > 8
        })
        .expect("preparation must only publish a lazy snapshot");
    assert_eq!(generator.walk_cache_usage(), before);
    assert!(generator.portal_cache().lock().unwrap().chunks.is_empty());
}

#[test]
fn prepared_navigation_keeps_only_pinned_collision_without_resource_amounts() {
    let generator = MapChunkGenerator::new([3; 32], 1, 128);
    let node = (0..4)
        .flat_map(|y| (0..4).map(move |x| (x, y)))
        .find_map(|(x, y)| generator.chunk(x, y).unwrap().resources.into_iter().next())
        .expect("fixture resource");
    let mut overlay = ResourceOverlay::default();
    overlay
        .deplete(&generator, node.id, node.initial_amount - 1)
        .unwrap();
    assert_eq!(overlay.changed_count(), 1);
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let retained = generator.prepared_navigation(&overlay).unwrap();
    assert_eq!(retained.overlay().changed_count(), 0);
    assert!(retained.overlay().blocks_node(node));

    overlay.deplete(&generator, node.id, 1).unwrap();
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let refreshed = generator.prepared_navigation(&overlay).unwrap();
    assert_eq!(refreshed.overlay().changed_count(), 0);
    assert!(!refreshed.overlay().blocks_node(node));
    assert!(
        retained.overlay().blocks_node(node),
        "a retained search must keep the collision captured before exhaustion"
    );
}

#[test]
fn chunk_preparation_cost_breakdown() {
    let cold = MapChunkGenerator::new([7; 32], 7, 64);
    let probes = std::cell::Cell::new(0);
    let started = Instant::now();
    cold.prepare_navigation(&ResourceOverlay::default(), &|| {
        probes.set(probes.get() + 1);
        false
    })
    .unwrap();
    eprintln!(
        "64-tile preparation: {:?}, {} cancellation probes, {} composed chunks",
        started.elapsed(),
        probes.get(),
        cold.walk_cache_usage().1
    );
    let generator = flat_terrain_with_water_wall(256, -1, -1);
    let mut overlay = ResourceOverlay::default();
    // Four border portals, with an interior obstacle forcing real Dijkstra
    // floods rather than the open rectangle's closed-form distance shortcut.
    overlay
        .add_footprint(aoe_core::TileRect::from_xywh(104, 102, 2, 16))
        .unwrap();
    let ground = Ground::new(&generator, &overlay, &|| false);
    let key = (3, 3);
    let started = Instant::now();
    generator.navigation_cells(key, &|| false).unwrap();
    let terrain = started.elapsed();
    let started = Instant::now();
    let grid = grid::Grid::new(&ground, key).unwrap();
    let edges = started.elapsed();
    let started = Instant::now();
    let _ = grid.components();
    let components = started.elapsed();
    let started = Instant::now();
    let chunk = ground.build_portals(key).unwrap();
    assert_eq!(chunk.portals.len(), 4);
    let portals = started.elapsed();
    let started = Instant::now();
    for portal in &chunk.portals {
        let _ = grid.costs(portal.tile, &|| false).unwrap();
    }
    eprintln!(
        "chunk {key:?}, {} portals: terrain={terrain:?}, edges={edges:?}, components={components:?}, portals={portals:?}, floods={:?}",
        chunk.portals.len(),
        started.elapsed()
    );
}

#[test]
fn a_nearby_component_query_only_composes_a_local_corridor() {
    let generator = flat_terrain_with_water_wall(16_000, -1, -1);
    let overlay = ResourceOverlay::default();
    let probes = std::cell::Cell::new(0);
    generator
        .prepare_navigation(&overlay, &|| {
            probes.set(probes.get() + 1);
            probes.get() > 8
        })
        .unwrap();
    let target = TileCoord::new(250, 100);
    assert_eq!(
        generator
            .navigation_destination(&overlay, TileCoord::new(100, 100), target, &|| false)
            .unwrap(),
        Some(target)
    );
    assert!(generator.walk_cache_usage().1 < 64);
    assert!(
        generator
            .portal_cache()
            .lock()
            .unwrap()
            .chunks
            .values()
            .all(|(_, chunk, _)| chunk.costs.get().is_none()),
        "connectivity does not need any inter-portal floods"
    );
    assert!(matches!(
        hierarchical_route(&generator, TileCoord::new(100, 100), target, 256).unwrap(),
        HierarchicalOutcome::Path { .. }
    ));
    assert!(
        generator
            .portal_cache()
            .lock()
            .unwrap()
            .chunks
            .values()
            .any(|(_, chunk, _)| chunk.costs.get().is_some()),
        "the first route retains its local costs"
    );
}

#[test]
fn a_blocked_goal_in_a_large_land_mass_still_only_visits_a_local_corridor() {
    let generator = flat_terrain_with_water_wall(16_000, -1, -1);
    let mut overlay = ResourceOverlay::default();
    overlay
        .add_footprint(aoe_core::TileRect::from_xywh(250, 100, 1, 1))
        .unwrap();
    let probes = std::cell::Cell::new(0);
    generator
        .prepare_navigation(&overlay, &|| {
            probes.set(probes.get() + 1);
            probes.get() > 8
        })
        .unwrap();
    assert_eq!(
        generator
            .navigation_destination(
                &overlay,
                TileCoord::new(100, 100),
                TileCoord::new(250, 100),
                &|| false
            )
            .unwrap(),
        Some(TileCoord::new(250, 99))
    );
    assert!(generator.walk_cache_usage().1 < 64);
}
