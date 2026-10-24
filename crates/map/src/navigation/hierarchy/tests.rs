mod budget;
mod preparation;
mod reference;

use super::{
    ChunkPortals, Ground, HierarchicalOutcome, Portal, PortalCache, entrance_runs,
    hierarchical_route,
};
use crate::navigation::long_routes_tests::flat_terrain_with_water_wall;
use crate::{CHUNK_TILES, MapChunkGenerator};
use aoe_core::TileCoord;
use std::sync::{Arc, OnceLock};

fn lower_bound(a: TileCoord, b: TileCoord) -> u64 {
    u64::from((a.x - b.x).unsigned_abs().max((a.y - b.y).unsigned_abs())) * 1_024
}

#[test]
fn tiles_in_one_chunk_need_no_waypoints() {
    let generator = flat_terrain_with_water_wall(64, -1, -1);
    let outcome = hierarchical_route(&generator, TileCoord::new(2, 2), TileCoord::new(10, 2), 64);
    assert_eq!(
        outcome,
        Ok(HierarchicalOutcome::Path {
            waypoints: Vec::new(),
            cost: 8 * 1_024
        })
    );
}

#[test]
fn a_wall_detour_goes_through_the_only_gateway() {
    let generator = flat_terrain_with_water_wall(64, 32, 5);
    let outcome = hierarchical_route(
        &generator,
        TileCoord::new(1, 32),
        TileCoord::new(62, 32),
        256,
    )
    .expect("route");
    let HierarchicalOutcome::Path { waypoints, cost } = outcome else {
        panic!("expected a path, got {outcome:?}");
    };
    assert!(
        waypoints.contains(&TileCoord::new(31, 5)) || waypoints.contains(&TileCoord::new(32, 5))
    );
    assert!(cost > lower_bound(TileCoord::new(1, 32), TileCoord::new(62, 32)));
}

#[test]
fn a_wall_without_a_gateway_is_proven_disconnected() {
    let generator = flat_terrain_with_water_wall(64, 32, -1);
    assert_eq!(
        hierarchical_route(
            &generator,
            TileCoord::new(1, 32),
            TileCoord::new(62, 32),
            256
        ),
        Ok(HierarchicalOutcome::Unreachable)
    );
}

#[test]
fn a_1600_tile_route_is_found_over_chunks_deterministically_and_within_bound() {
    let generator = flat_terrain_with_water_wall(4_096, -1, -1);
    let (origin, destination) = (TileCoord::new(100, 200), TileCoord::new(1_700, 900));
    let first = hierarchical_route(&generator, origin, destination, 2_048).expect("route");
    let again = hierarchical_route(
        &flat_terrain_with_water_wall(4_096, -1, -1),
        origin,
        destination,
        2_048,
    )
    .expect("route");
    assert_eq!(first, again, "a cold cache must give the same answer");
    let HierarchicalOutcome::Path { waypoints, cost } = first else {
        panic!("expected a path, got {first:?}");
    };
    assert!(waypoints.len() >= 2 * 49 && waypoints.len() % 2 == 0);
    assert!(cost >= lower_bound(origin, destination));
    for entrance in waypoints.chunks(2) {
        let (a, b) = (entrance[0], entrance[1]);
        assert_eq!((a.x - b.x).abs() + (a.y - b.y).abs(), 1);
        assert_ne!(
            (a.x.div_euclid(32), a.y.div_euclid(32)),
            (b.x.div_euclid(32), b.y.div_euclid(32))
        );
    }
    assert_eq!(
        hierarchical_route(&generator, origin, destination, 3),
        Ok(HierarchicalOutcome::Bounded)
    );
}

#[test]
fn blocked_endpoints_are_invalid() {
    let generator = flat_terrain_with_water_wall(64, 32, 5);
    assert_eq!(
        hierarchical_route(
            &generator,
            TileCoord::new(32, 20),
            TileCoord::new(62, 32),
            16
        ),
        Ok(HierarchicalOutcome::Invalid)
    );
}

#[test]
fn a_height_step_inside_a_run_of_open_pairs_gives_each_part_its_own_portal() {
    let open = [true; 8];
    let mut linked = [true; 8];
    assert_eq!(entrance_runs(&open, &linked), vec![(0, 8)]);
    linked[3] = false;
    assert_eq!(entrance_runs(&open, &linked), vec![(0, 3), (3, 8)]);
    // A closed pair still ends a run, and a split at the first pair is no split.
    let open = [true, true, false, true, true, true];
    let linked = [false, true, true, false, true, true];
    assert_eq!(entrance_runs(&open, &linked), vec![(0, 2), (3, 6)]);
    assert!(entrance_runs(&[false; 4], &[false; 4]).is_empty());
}

fn portals_of(count: usize) -> Arc<ChunkPortals> {
    let at = TileCoord::new(0, 0);
    Arc::new(ChunkPortals {
        portals: vec![
            Portal {
                tile: at,
                cross: at,
                crossing_cost: 0,
                bidirectional: true,
            };
            count
        ],
        costs: OnceLock::new(),
        labels: super::grid::Labels::Empty,
        components: 0,
        links: Vec::new(),
    })
}

#[test]
fn the_portal_cache_is_bounded_by_bytes_and_evicts_the_oldest_chunk() {
    let one = portals_of(40).bytes();
    let mut cache = PortalCache::with_limit(one * 5);
    for x in 0..5 {
        cache.insert((x, 0), [0; 32], portals_of(40));
    }
    assert_eq!((cache.len(), cache.retained_bytes()), (5, one * 5));
    // Touching chunk 0 makes chunk 1 the least recently used.
    assert!(cache.chunk((0, 0), [0; 32]).is_some());
    cache.insert((9, 9), [0; 32], portals_of(40));
    assert!(cache.retained_bytes() <= one * 5);
    assert!(
        cache.chunk((0, 0), [0; 32]).is_some(),
        "the touched chunk survives"
    );
    assert!(
        cache.chunk((1, 0), [0; 32]).is_none(),
        "the oldest chunk is evicted"
    );
    assert!(cache.chunk((9, 9), [0; 32]).is_some());
    // Many large entries never exceed the limit, whatever their number.
    for x in 0..200 {
        cache.insert((x, 5), [0; 32], portals_of(40));
        assert!(cache.retained_bytes() <= one * 5);
    }
}

#[test]
fn depleted_resource_nodes_do_not_close_tiles_in_the_current_graph() {
    let generator = MapChunkGenerator::new([7; 32], 0, 4_096);
    let mut overlay = crate::ResourceOverlay::default();
    let mut checked = 0;
    for y in 0..CHUNK_TILES * 4 {
        for x in 0..CHUNK_TILES * 4 {
            let tile = TileCoord::new(x, y);
            let cell = generator
                .walk_at_with_cancel(tile, &|| false)
                .expect("cell")
                .expect("inside the map");
            if let Some(node) = cell.node {
                checked += 1;
                overlay
                    .deplete(&generator, node.id, node.initial_amount)
                    .unwrap();
                let ground = Ground::new(&generator, &overlay, &|| false);
                assert_eq!(ground.is_open(tile), Ok(cell.passable), "{tile:?}");
            }
        }
    }
    assert!(checked > 0, "the window must contain resource nodes");
}

#[test]
fn incremental_guidance_is_budgeted_and_independent_of_cache_warmth() {
    use super::incremental::Search;
    let generator = flat_terrain_with_water_wall(64, 32, 5);
    let run = |slice| {
        let mut search = Search::new(TileCoord::new(1, 32), TileCoord::new(62, 32), 256);
        let mut work = 0;
        loop {
            let mut remaining = slice;
            let outcome = search.poll(&generator, &mut remaining, &|| false).unwrap();
            work += slice - remaining;
            assert!(search.retained_entries() <= 65_536);
            if let Some(outcome) = outcome {
                return (outcome, work);
            }
            assert!(work < 20_000_000);
        }
    };
    let cold = run(16_384);
    assert!(matches!(cold.0, HierarchicalOutcome::Path { .. }));
    assert_eq!(cold, run(7));
}

#[test]
fn incremental_guidance_cancels_without_spending_its_budget() {
    use super::incremental::Search;
    let generator = flat_terrain_with_water_wall(64, 32, 5);
    let mut search = Search::new(TileCoord::new(1, 32), TileCoord::new(62, 32), 256);
    let mut budget = 16_384;
    assert_eq!(
        search.poll(&generator, &mut budget, &|| true),
        Err(crate::EnvironmentPageError::Cancelled)
    );
    assert_eq!(budget, 16_384);
}

#[test]
fn prepared_components_certify_the_nearest_reachable_tile_and_track_barriers() {
    use crate::ResourceOverlay;
    use aoe_core::TileRect;
    let generator = flat_terrain_with_water_wall(64, 32, -1);
    let mut overlay = ResourceOverlay::default();
    let origin = TileCoord::new(1, 20);
    let target = TileCoord::new(62, 20);
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    assert_eq!(
        generator
            .navigation_destination(&overlay, origin, target, &|| false)
            .unwrap(),
        Some(TileCoord::new(31, 20))
    );
    let wall = TileRect::from_xywh(16, 0, 1, 64);
    overlay.add_footprint(wall).unwrap();
    assert_eq!(
        generator
            .navigation_destination(&overlay, origin, target, &|| false)
            .unwrap(),
        Some(TileCoord::new(15, 20))
    );
    overlay.remove_footprint(wall);
    assert_eq!(
        generator
            .navigation_destination(&overlay, origin, target, &|| false)
            .unwrap(),
        Some(TileCoord::new(31, 20))
    );
}

#[test]
fn a_warm_abstract_search_charges_graph_work_instead_of_tile_floods() {
    use super::incremental::Search;
    use crate::ResourceOverlay;
    let generator = flat_terrain_with_water_wall(256, -1, -1);
    let overlay = ResourceOverlay::default();
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let mut search = Search::new(TileCoord::new(1, 20), TileCoord::new(250, 20), 256);
    let mut remaining = 32_768;
    let outcome = search.poll(&generator, &mut remaining, &|| false).unwrap();
    assert!(
        matches!(outcome, Some(HierarchicalOutcome::Path { .. })),
        "{outcome:?}"
    );
}

#[test]
fn depleted_gateway_and_restored_overlay_update_only_the_current_graph() {
    use crate::{ResourceKind, ResourceNode, ResourceOverlay};
    let generator = flat_terrain_with_water_wall(64, 32, 5);
    let mut overlay = ResourceOverlay::default();
    let (origin, target) = (TileCoord::new(1, 20), TileCoord::new(62, 20));
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    assert_eq!(
        generator
            .navigation_destination(&overlay, origin, target, &|| false)
            .unwrap(),
        Some(target)
    );
    let tree = ResourceNode::starting(TileCoord::new(32, 5), ResourceKind::Wood).unwrap();
    overlay.insert_starting_resource(&generator, tree).unwrap();
    assert_eq!(
        generator
            .navigation_destination(&overlay, origin, target, &|| false)
            .unwrap(),
        Some(TileCoord::new(31, 20))
    );
    overlay
        .deplete(&generator, tree.id, tree.initial_amount)
        .unwrap();
    let restored =
        ResourceOverlay::from_snapshot(&overlay.snapshot([7; 32]), [7; 32], &generator, &|| false)
            .unwrap();
    assert_eq!(overlay, restored);
    assert_eq!(
        generator
            .navigation_destination(&restored, origin, target, &|| false)
            .unwrap(),
        Some(target)
    );
}

#[test]
fn cancelling_preparation_does_not_publish_partial_land_components() {
    use crate::ResourceOverlay;
    use std::cell::Cell;
    let generator = flat_terrain_with_water_wall(64, 32, 5);
    let overlay = ResourceOverlay::default();
    // Startup only captures collision now. Cancel that publication first,
    // then cancel the deferred terrain work after its original 100 probes.
    let result = generator.prepare_navigation(&overlay, &|| true);
    assert_eq!(result, Err(crate::EnvironmentPageError::Cancelled));
    assert_eq!(
        generator
            .navigation_destination(
                &overlay,
                TileCoord::new(1, 20),
                TileCoord::new(62, 20),
                &|| false
            )
            .unwrap(),
        None
    );
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let calls = Cell::new(0);
    let result = generator.navigation_destination(
        &overlay,
        TileCoord::new(1, 20),
        TileCoord::new(62, 20),
        &|| {
            calls.set(calls.get() + 1);
            calls.get() > 100
        },
    );
    assert_eq!(result, Err(crate::EnvironmentPageError::Cancelled));
    assert_eq!(
        generator
            .navigation_destination(
                &overlay,
                TileCoord::new(1, 20),
                TileCoord::new(62, 20),
                &|| false
            )
            .unwrap(),
        Some(TileCoord::new(62, 20))
    );
}

#[test]
fn opening_ground_during_an_abstract_search_keeps_its_portal_snapshot_valid() {
    use super::incremental::Search;
    use crate::{ResourceKind, ResourceNode, ResourceOverlay};
    let generator = flat_terrain_with_water_wall(64, -1, -1);
    let mut overlay = ResourceOverlay::default();
    let tree = ResourceNode::starting(TileCoord::new(31, 10), ResourceKind::Wood).unwrap();
    overlay.insert_starting_resource(&generator, tree).unwrap();
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let mut search = Search::new(TileCoord::new(1, 20), TileCoord::new(62, 20), 256);
    let mut budget = (32 * 32 * 9 + 32 * 8) * 2;
    assert_eq!(
        search
            .poll_overlay(&generator, &overlay, &mut budget, &|| false)
            .unwrap(),
        None
    );
    overlay
        .deplete(&generator, tree.id, tree.initial_amount)
        .unwrap();
    // Removing the border tree merges two runs and changes their midpoint.
    generator.prepare_navigation(&overlay, &|| false).unwrap();
    let mut budget = 32_768;
    let outcome = search
        .poll_overlay(&generator, &overlay, &mut budget, &|| false)
        .unwrap();
    assert!(
        matches!(outcome, Some(HierarchicalOutcome::Path { .. })),
        "{outcome:?}"
    );
}
