use super::super::*;
use crate::navigation::long_routes_tests::flat_terrain_with_water_wall;

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
