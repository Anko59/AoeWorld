use super::super::*;
use crate::navigation::long_routes_tests::flat_terrain_with_water_wall;
use std::sync::{Arc, Mutex};

fn snapshot(source: &MapChunkGenerator) -> NavigationSnapshot {
    let side = source.navigation_chunk_side();
    let mut bytes = Vec::new();
    let mut records = Vec::new();
    for y in 0..side {
        for x in 0..side {
            let chunk = source
                .prepare_static_navigation_chunk(x, y, &|| false)
                .unwrap();
            let start = bytes.len();
            bytes.extend(chunk.encode().unwrap());
            records.push(start..bytes.len());
        }
    }
    NavigationSnapshot::from_storage(side, Arc::new(bytes), records, &|| false).unwrap()
}

#[derive(Debug, Default)]
struct Probe(Arc<Mutex<Vec<crate::NavigationPhase>>>);
struct Span;
impl crate::NavigationSpan for Span {}
impl crate::NavigationProbe for Probe {
    fn begin(&self, phase: crate::NavigationPhase) -> Box<dyn crate::NavigationSpan> {
        self.0.lock().unwrap().push(phase);
        Box::new(Span)
    }
}

#[test]
fn packed_snapshot_walks_components_costs_and_dynamic_routes_match_lazy() {
    let source = flat_terrain_with_water_wall(64, 32, 5);
    let loaded = flat_terrain_with_water_wall(64, 32, 5);
    let data = snapshot(&source);
    assert!(
        data.heap_bytes() <= 8 * 1024,
        "mapped tables must retain only an offset index"
    );
    loaded.install_navigation_snapshot(Arc::new(data)).unwrap();
    let probe = Arc::new(Probe::default());
    loaded.set_navigation_probe(probe.clone());
    for y in 0..64 {
        for x in 0..64 {
            let tile = TileCoord::new(x, y);
            assert_eq!(
                loaded.walk_at_with_cancel(tile, &|| false),
                source.walk_at_with_cancel(tile, &|| false)
            );
        }
    }
    let mut overlay = ResourceOverlay::default();
    for (origin, goal) in [
        (TileCoord::new(1, 32), TileCoord::new(62, 32)),
        (TileCoord::new(5, 5), TileCoord::new(62, 5)),
    ] {
        loaded.prepare_navigation(&overlay, &|| false).unwrap();
        source.prepare_navigation(&overlay, &|| false).unwrap();
        assert_eq!(
            loaded.navigation_destination(&overlay, origin, goal, &|| false),
            source.navigation_destination(&overlay, origin, goal, &|| false)
        );
        assert_eq!(
            hierarchical_route(&loaded, origin, goal, 256),
            hierarchical_route(&source, origin, goal, 256)
        );
    }
    assert_eq!(loaded.walk_cache_usage(), (0, 0));
    assert_eq!(loaded.portal_cache_usage(), (0, 0));
    use crate::NavigationPhase::*;
    assert!(
        probe
            .0
            .lock()
            .unwrap()
            .iter()
            .all(|phase| !matches!(phase, Terrain | TerrainPoint | Portals | PortalCosts)),
        "installed static queries must not construct navigation"
    );
    overlay
        .add_footprint(aoe_core::TileRect::from_xywh(30, 5, 4, 1))
        .unwrap();
    let (origin, goal) = (TileCoord::new(1, 32), TileCoord::new(62, 32));
    assert_eq!(
        hierarchical_route_with_overlay(&loaded, &overlay, origin, goal, 256),
        hierarchical_route_with_overlay(&source, &overlay, origin, goal, 256)
    );
    assert_eq!(
        loaded.walk_cache_usage(),
        (0, 0),
        "overlay rebuilds still use stored terrain"
    );
}

#[test]
fn packed_snapshot_rejects_partial_duplicate_out_of_bounds_and_cancelled_records() {
    let source = flat_terrain_with_water_wall(64, -1, -1);
    let chunk = source
        .prepare_static_navigation_chunk(0, 0, &|| false)
        .unwrap()
        .encode()
        .unwrap();
    let bytes = Arc::new(chunk.clone());
    let end = chunk.len();
    assert!(
        NavigationSnapshot::from_storage(2, bytes.clone(), vec![0..end; 3], &|| false).is_err()
    );
    assert!(
        NavigationSnapshot::from_storage(2, bytes.clone(), vec![0..end; 4], &|| false).is_err()
    );
    let outside = std::iter::once(0..end + 1).collect();
    let only = std::iter::once(0..end).collect();
    assert!(NavigationSnapshot::from_storage(1, bytes.clone(), outside, &|| false).is_err());
    assert!(NavigationSnapshot::from_storage(1, bytes, only, &|| true).is_err());
    for end in 0..chunk.len() {
        assert!(NavigationChunk::decode(&chunk[..end]).is_err());
    }
    let mut trailing = chunk;
    trailing.push(0);
    assert!(NavigationChunk::decode(&trailing).is_err());
    let mut malformed = source
        .prepare_static_navigation_chunk(0, 0, &|| false)
        .unwrap()
        .encode()
        .unwrap();
    // Reject impossible dimensions before layout arithmetic, including on wasm32.
    malformed[12..14].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(NavigationChunk::decode(&malformed).is_err());
}

#[test]
fn background_installation_targets_do_not_retain_retired_worlds() {
    let source = flat_terrain_with_water_wall(64, -1, -1);
    let target = source.navigation_target();
    assert!(target.is_alive());
    let data = Arc::new(snapshot(&source));
    target.install(data.clone()).unwrap();
    assert!(Arc::ptr_eq(&source.navigation_snapshot().unwrap(), &data));
    drop(source);
    assert!(!target.is_alive());
    target.install(data).unwrap();
}
