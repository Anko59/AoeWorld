use super::super::*;
use crate::navigation::long_routes_tests::flat_terrain_with_water_wall;

#[test]
fn bridge_updates_persisted_components_portals_and_axis_edges_locally() {
    bridge_updates_static_navigation(false);
}

#[test]
fn bridge_updates_packed_navigation_after_construction_and_destruction() {
    bridge_updates_static_navigation(true);
}

fn bridge_updates_static_navigation(packed: bool) {
    use crate::{RoutePlanner, RoutePlannerPoll, RouteTerrain};
    let source = flat_terrain_with_water_wall(64, 32, -1);
    let chunks: Vec<_> = (0..2)
        .flat_map(|y| (0..2).map(move |x| (x, y)))
        .map(|(x, y)| {
            source
                .prepare_static_navigation_chunk(x, y, &|| false)
                .unwrap()
        })
        .collect();
    let terrain = flat_terrain_with_water_wall(64, 32, -1);
    if packed {
        // The combined water/navp path reads immutable packed cells while the
        // bridge overlay invalidates only affected components and portal rows.
        let mut bytes = Vec::new();
        let records = chunks
            .iter()
            .map(|chunk| {
                let start = bytes.len();
                bytes.extend(chunk.encode().unwrap());
                start..bytes.len()
            })
            .collect();
        let snapshot =
            NavigationSnapshot::from_storage(2, Arc::new(bytes), records, &|| false).unwrap();
        terrain
            .install_navigation_snapshot(Arc::new(snapshot))
            .unwrap();
    } else {
        terrain.install_static_navigation(chunks).unwrap();
    }
    assert_eq!(terrain.walk_cache_usage(), (0, 0));
    let mut overlay = ResourceOverlay::default();
    let (from, to) = (TileCoord::new(29, 20), TileCoord::new(35, 20));
    terrain.prepare_navigation(&overlay, &|| false).unwrap();
    assert_ne!(
        terrain
            .navigation_destination(&overlay, from, to, &|| false)
            .unwrap(),
        Some(to)
    );
    let rect = aoe_core::TileRect::from_xywh(31, 20, 3, 1);
    let snapshot = overlay.navigation_snapshot();
    let distant_stamp = overlay.navigation_key((3, 3));
    let span = aoe_core::BridgeSpan::new(rect.min, TileCoord::new(rect.max.x - 1, rect.max.y - 1))
        .unwrap();
    overlay.add_bridge(span).unwrap();
    assert_eq!(
        snapshot.bridge_connections(TileCoord::new(32, 20)),
        None,
        "pinned searches retain their own deck state"
    );
    assert_eq!(
        overlay.navigation_key((3, 3)),
        distant_stamp,
        "unaffected chunk fingerprints stay reusable"
    );
    terrain.prepare_navigation(&overlay, &|| false).unwrap();
    assert_eq!(
        terrain
            .navigation_destination(&overlay, from, to, &|| false)
            .unwrap(),
        Some(to)
    );
    let deck = TileCoord::new(32, 20);
    assert!(matches!(
        super::super::hierarchical_route_with_overlay(&terrain, &overlay, from, to, 256).unwrap(),
        HierarchicalOutcome::Path { .. }
    ));
    assert!(matches!(
        crate::find_path_with_overlay(&terrain, &overlay, from, to, 4096),
        crate::MovementOutcome::Path(_)
    ));
    assert!(terrain.route_walkable(&overlay, deck, &|| false).unwrap());
    assert_eq!(
        overlay.bridge_edge(TileCoord::new(31, 20), deck),
        Some(true)
    );
    assert_eq!(
        overlay.bridge_edge(TileCoord::new(31, 19), deck),
        Some(false)
    );
    assert_eq!(
        overlay.bridge_edge(TileCoord::new(32, 19), deck),
        Some(false)
    );
    let mut planner = RoutePlanner::new(from, to, 65_536);
    let mut crossed = false;
    for _ in 0..128 {
        match planner.poll(&terrain, &overlay, 2_048, &|| false) {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                crossed |= path.tiles.contains(&deck);
                if planner.is_terminal() {
                    break;
                }
            }
            other => panic!("bridge route failed: {other:?}"),
        }
    }
    assert!(crossed, "the route must cross the only connecting bridge");
    overlay.remove_bridge(span);
    terrain.prepare_navigation(&overlay, &|| false).unwrap();
    assert!(!terrain.route_walkable(&overlay, deck, &|| false).unwrap());
    assert_ne!(
        terrain
            .navigation_destination(&overlay, from, to, &|| false)
            .unwrap(),
        Some(to)
    );
    assert_eq!(
        terrain.walk_cache_usage(),
        (0, 0),
        "dynamic graphs use persisted walk data"
    );
}

#[test]
fn persisted_static_navigation_matches_cold_routes_and_dynamic_collision() {
    let original = flat_terrain_with_water_wall(64, 32, 5);
    let loaded = flat_terrain_with_water_wall(64, 32, 5);
    let mut chunks = Vec::new();
    for y in 0..2 {
        for x in 0..2 {
            let chunk = original
                .prepare_static_navigation_chunk(x, y, &|| false)
                .unwrap();
            chunks.push(crate::NavigationChunk::decode(&chunk.encode().unwrap()).unwrap());
        }
    }
    loaded.install_static_navigation(chunks).unwrap();
    for y in 0..64 {
        for x in 0..64 {
            let tile = TileCoord::new(x, y);
            assert_eq!(
                loaded.object_at_with_cancel(tile, &|| false),
                original.object_at_with_cancel(tile, &|| false)
            );
            assert_eq!(
                loaded.walk_at_with_cancel(tile, &|| false),
                original.walk_at_with_cancel(tile, &|| false)
            );
        }
    }
    let (origin, goal) = (TileCoord::new(1, 32), TileCoord::new(62, 32));
    assert_eq!(
        hierarchical_route(&loaded, origin, goal, 256),
        hierarchical_route(&original, origin, goal, 256)
    );
    assert_eq!(
        loaded.walk_cache_usage(),
        (0, 0),
        "loaded navigation never composes terrain chunks"
    );
    let mut overlay = ResourceOverlay::default();
    overlay
        .add_footprint(aoe_core::TileRect::from_xywh(30, 5, 4, 1))
        .unwrap();
    loaded.prepare_navigation(&overlay, &|| false).unwrap();
    original.prepare_navigation(&overlay, &|| false).unwrap();
    assert_eq!(
        loaded.navigation_destination(&overlay, origin, goal, &|| false),
        original.navigation_destination(&overlay, origin, goal, &|| false)
    );
}

#[test]
fn static_navigation_rejects_partial_duplicate_and_malformed_data() {
    let generator = flat_terrain_with_water_wall(64, -1, -1);
    assert!(generator.install_static_navigation(Vec::new()).is_err());
    let chunk = generator
        .prepare_static_navigation_chunk(0, 0, &|| false)
        .unwrap();
    let bytes = chunk.encode().unwrap();
    assert!(crate::NavigationChunk::decode(&bytes[..bytes.len() / 2]).is_err());
    let duplicates = (0..4)
        .map(|_| crate::NavigationChunk::decode(&bytes).unwrap())
        .collect();
    assert!(generator.install_static_navigation(duplicates).is_err());
    assert!(
        generator
            .prepare_static_navigation_chunk(0, 0, &|| true)
            .is_err()
    );
}

#[derive(Debug)]
struct UnavailableSource(std::sync::atomic::AtomicUsize);
impl crate::EnvironmentPageProvider for UnavailableSource {
    fn page(
        &self,
        _: crate::EnvironmentPageKey,
        _: &dyn Fn() -> bool,
    ) -> Result<Arc<crate::EnvironmentPage>> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err(EnvironmentPageError::Unavailable)
    }
}

#[test]
fn dynamic_chunk_rebuilds_use_only_the_loaded_static_snapshot() {
    let source = flat_terrain_with_water_wall(64, -1, -1);
    let mut chunks = Vec::new();
    for y in 0..2 {
        for x in 0..2 {
            chunks.push(
                source
                    .prepare_static_navigation_chunk(x, y, &|| false)
                    .unwrap(),
            );
        }
    }
    // Once prepared, navigation must remain independent of source residency:
    // this provider refuses every read, including coarse water shortcuts.
    let provider = Arc::new(UnavailableSource(std::sync::atomic::AtomicUsize::new(0)));
    let field = crate::FieldPyramid {
        levels: vec![crate::PyramidLevel {
            samples_per_axis: 1,
            ordered_page_root: [1; 32],
        }],
    };
    let environment = crate::PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: field.clone(),
        water: Some(field),
        ..Default::default()
    };
    let loaded = MapChunkGenerator::new([0; 32], 0, 64)
        .with_page_provider(
            crate::Ratio::new(1, 1).unwrap(),
            environment,
            provider.clone(),
        )
        .unwrap();
    loaded.install_static_navigation(chunks).unwrap();
    let mut overlay = ResourceOverlay::default();
    overlay
        .add_footprint(aoe_core::TileRect::from_xywh(30, 0, 1, 64))
        .unwrap();
    loaded.prepare_navigation(&overlay, &|| false).unwrap();
    assert!(
        loaded
            .navigation_destination(
                &overlay,
                TileCoord::new(1, 10),
                TileCoord::new(60, 10),
                &|| false
            )
            .is_ok()
    );
    assert_eq!(provider.0.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert_eq!(loaded.walk_cache_usage(), (0, 0));
}

#[test]
fn diagonal_bridge_updates_persisted_chunk_corner_routes_and_destruction() {
    use crate::{RoutePlanner, RoutePlannerPoll};
    use aoe_core::BridgeSpan;
    let terrain = flat_terrain_with_water_wall(64, 32, -1);
    let chunks = (0..2)
        .flat_map(|y| (0..2).map(move |x| (x, y)))
        .map(|(x, y)| {
            terrain
                .prepare_static_navigation_chunk(x, y, &|| false)
                .unwrap()
        })
        .collect();
    terrain.install_static_navigation(chunks).unwrap();
    let mut overlay = ResourceOverlay::default();
    let (from, to) = (TileCoord::new(30, 30), TileCoord::new(34, 34));
    let span = BridgeSpan::new(TileCoord::new(31, 31), TileCoord::new(33, 33)).unwrap();
    let before = overlay.navigation_snapshot();
    overlay.add_bridge_foundation(span).unwrap();
    assert!(
        !overlay.blocks_tile(TileCoord::new(31, 33)),
        "bounding-box land stays free"
    );
    assert!(overlay.complete_bridge(span));
    terrain.prepare_navigation(&overlay, &|| false).unwrap();
    assert_eq!(
        terrain
            .navigation_destination(&overlay, from, to, &|| false)
            .unwrap(),
        Some(to)
    );
    assert_eq!(before.bridge_connections(TileCoord::new(32, 32)), None);
    assert_eq!(
        overlay.bridge_edge(TileCoord::new(31, 31), TileCoord::new(32, 32)),
        Some(true)
    );
    assert_eq!(
        overlay.bridge_edge(TileCoord::new(31, 32), TileCoord::new(32, 32)),
        Some(false)
    );
    assert!(matches!(
        crate::find_path_with_overlay(&terrain, &overlay, from, to, 4096),
        crate::MovementOutcome::Path(_)
    ));
    let mut planner = RoutePlanner::new(from, to, 4096);
    assert!(matches!(
        planner.poll(&terrain, &overlay, 4096, &|| false),
        RoutePlannerPoll::Path(_)
    ));
    overlay.remove_bridge(span);
    terrain.prepare_navigation(&overlay, &|| false).unwrap();
    assert_ne!(
        terrain
            .navigation_destination(&overlay, from, to, &|| false)
            .unwrap(),
        Some(to)
    );
}
