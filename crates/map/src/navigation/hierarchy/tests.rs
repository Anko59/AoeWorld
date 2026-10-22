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
                cross: at
            };
            count
        ],
        costs: OnceLock::new(),
    })
}

#[test]
fn the_portal_cache_is_bounded_by_bytes_and_evicts_the_oldest_chunk() {
    let one = portals_of(40).bytes();
    let mut cache = PortalCache::with_limit(one * 5);
    for x in 0..5 {
        cache.insert((x, 0), portals_of(40));
    }
    assert_eq!((cache.len(), cache.retained_bytes()), (5, one * 5));
    // Touching chunk 0 makes chunk 1 the least recently used.
    assert!(cache.chunk((0, 0)).is_some());
    cache.insert((9, 9), portals_of(40));
    assert!(cache.retained_bytes() <= one * 5);
    assert!(cache.chunk((0, 0)).is_some(), "the touched chunk survives");
    assert!(cache.chunk((1, 0)).is_none(), "the oldest chunk is evicted");
    assert!(cache.chunk((9, 9)).is_some());
    // Many large entries never exceed the limit, whatever their number.
    for x in 0..200 {
        cache.insert((x, 5), portals_of(40));
        assert!(cache.retained_bytes() <= one * 5);
    }
}

#[test]
fn a_resource_node_does_not_close_its_tile_in_the_static_graph() {
    let generator = MapChunkGenerator::new([7; 32], 0, 4_096);
    let ground = Ground(&generator);
    let mut checked = 0;
    for y in 0..CHUNK_TILES * 4 {
        for x in 0..CHUNK_TILES * 4 {
            let tile = TileCoord::new(x, y);
            let cell = generator
                .walk_at_with_cancel(tile, &|| false)
                .expect("cell")
                .expect("inside the map");
            if cell.node.is_some() {
                checked += 1;
                assert_eq!(ground.is_open(tile), Ok(cell.passable), "{tile:?}");
            }
        }
    }
    assert!(checked > 0, "the window must contain resource nodes");
}
