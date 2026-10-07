use super::*;
use crate::{EdgePassability, ResourceOverlay, SurfaceKind};

fn old_predicate(from: Tile, to: Tile) -> EdgePassability {
    if !from.passable
        || !to.passable
        || !from.surface.walkable()
        || !to.surface.walkable()
        || (i32::from(from.game_height_level) - i32::from(to.game_height_level)).abs() > 1
    {
        EdgePassability::Blocked
    } else {
        EdgePassability::Passable
    }
}

#[derive(Debug)]
struct PhysicalProvider {
    elevation: ElevationPage,
    water: WaterPage,
}

impl crate::EnvironmentPageProvider for PhysicalProvider {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        match key.layer {
            crate::PageLayer::Elevation => {
                Ok(Arc::new(EnvironmentPage::Elevation(self.elevation.clone())))
            }
            crate::PageLayer::Water => Ok(Arc::new(EnvironmentPage::Water(self.water.clone()))),
            _ => Provider { error: None }.page(key, cancelled),
        }
    }
}

fn physical_sources() -> (MapChunkGenerator, MapChunkGenerator) {
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 4,
        height: 4,
        geographic_height_centimeters: [0, 0, 3_000, 30_000].repeat(4),
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 4,
        height: 4,
        ocean_coverage_percent: vec![0; 16],
        inland_coverage_percent: [vec![100; 4], vec![0; 12]].concat(),
    };
    let mut dense = flat(1, 16);
    dense.elevation = Some(Arc::new(PreparedElevation {
        samples_per_axis: 4,
        compression: Ratio::new(30, 1).unwrap(),
        sampling_recipe: crate::LANDSCAPE_GENERATION_RECIPE_VERSION,
        pages: [((0, 0), elevation.clone())].into(),
    }));
    // Keep vegetation procedural on both paths; physical terrain is independent
    // of the ecological palette and object mask.
    dense.biome = None;
    dense.water = Some(Arc::new(PreparedWater::new(
        4,
        [((0, 0), water.clone())].into(),
    )));
    let field = FieldPyramid {
        levels: vec![PyramidLevel {
            samples_per_axis: 4,
            ordered_page_root: [1; 32],
        }],
    };
    let mut source = dense.clone();
    source.provider = Some(Arc::new(PhysicalProvider { elevation, water }));
    source.provider_environment = Some(Arc::new(PreparedEnvironment {
        samples_per_axis: 4,
        elevation: field.clone(),
        water: Some(field),
        ..PreparedEnvironment::default()
    }));
    source.provider_compression = Some(Ratio::new(30, 1).unwrap());
    (dense, source)
}

#[test]
fn base_edges_match_old_composed_physics_on_fallback_dense_and_provider() {
    let (dense, source) = physical_sources();
    let fallback = MapChunkGenerator::new([17; 32], 1, 16)
        .with_elevation_sampling_recipe(crate::LANDSCAPE_GENERATION_RECIPE_VERSION);
    let mut saw_water = false;
    let mut saw_cliff = false;
    let mut saw_height_gap = false;
    let mut saw_passable = false;
    for generator in [&fallback, &dense, &source] {
        for y in 0..16 {
            for x in 0..16 {
                let tile = TileCoord::new(x, y);
                let composed = generator
                    .tile_at_with_cancel(tile, &|| false)
                    .unwrap()
                    .unwrap();
                let base = generator
                    .base_physical_tile_with_cancel(tile, &|| false)
                    .unwrap()
                    .unwrap();
                assert_eq!(composed.passable, base.passable);
                assert_eq!(composed.surface, base.surface);
                assert_eq!(composed.game_height_level, base.game_height_level);
                saw_water |= base.water != WaterKind::None;
                saw_cliff |= base.surface.kind == SurfaceKind::Cliff;
                for (dx, dy) in [(1, 0), (0, 1), (1, 1), (-1, 1)] {
                    let next = TileCoord::new(x + dx, y + dy);
                    let Some(other) = generator.tile_at_with_cancel(next, &|| false).unwrap()
                    else {
                        continue;
                    };
                    saw_height_gap |= (i32::from(composed.game_height_level)
                        - i32::from(other.game_height_level))
                    .abs()
                        > 1;
                    let expected = old_predicate(composed, other);
                    saw_passable |= expected == EdgePassability::Passable;
                    assert_eq!(generator.edge_between(tile, next), expected);
                    assert_eq!(
                        generator.edge_between_with_cancel(tile, next, &|| false),
                        Ok(expected)
                    );
                }
            }
        }
    }
    assert!(saw_water && saw_cliff && saw_height_gap && saw_passable);
    for y in 0..16 {
        for x in 0..16 {
            let tile = TileCoord::new(x, y);
            assert_eq!(
                dense.base_physical_tile_with_cancel(tile, &|| false),
                source.base_physical_tile_with_cancel(tile, &|| false)
            );
        }
    }
}

#[test]
fn combined_pairs_preserve_resources_and_external_depleted_overlay_filtering() {
    for generator in [flat(1, 256), provider(None)] {
        let mut saw_node = false;
        let mut saw_empty = false;
        for y in 0..32 {
            for x in 0..32 {
                let tile = TileCoord::new(x, y);
                let old_tile = generator.tile_at_with_cancel(tile, &|| false).unwrap();
                let old_node = generator.object_at_with_cancel(tile, &|| false).unwrap();
                let pair = generator
                    .tile_and_node_with_cancel(tile, &|| false)
                    .unwrap();
                assert_eq!(pair, old_tile.map(|sample| (sample, old_node)));
                let (sample, node) = pair.unwrap();
                let mut overlay = ResourceOverlay::default();
                assert_eq!(
                    sample.passable && node.is_none_or(|node| !overlay.blocks_node(node)),
                    old_tile.unwrap().passable
                        && old_node.is_none_or(|node| !overlay.blocks_node(node))
                );
                if let Some(node) = node {
                    if !saw_node {
                        assert!(overlay.blocks_node(node));
                        overlay
                            .deplete(&generator, node.id, node.initial_amount)
                            .unwrap();
                        assert!(!overlay.blocks_node(node));
                        assert_eq!(
                            generator
                                .tile_and_node_with_cancel(tile, &|| false)
                                .unwrap(),
                            pair
                        );
                        assert!(sample.passable && !overlay.blocks_node(node));
                    }
                    saw_node = true;
                } else {
                    saw_empty = true;
                }
            }
        }
        assert!(saw_node && saw_empty);
    }
}

#[test]
fn physical_and_combined_queries_propagate_errors_and_preserve_edge_bounds() {
    let tile = TileCoord::new(10, 10);
    let next = TileCoord::new(11, 10);
    for generator in [flat(1, 256), provider(None)] {
        assert_eq!(
            generator.base_physical_tile_with_cancel(tile, &|| true),
            Err(EnvironmentPageError::Cancelled)
        );
        assert_eq!(
            generator.tile_and_node_with_cancel(tile, &|| true),
            Err(EnvironmentPageError::Cancelled)
        );
        assert_eq!(
            generator.edge_between_with_cancel(tile, next, &|| true),
            Err(EnvironmentPageError::Cancelled)
        );
        assert_eq!(
            generator.base_physical_tile_with_cancel(TileCoord::new(-1, 0), &|| false),
            Ok(None)
        );
        assert_eq!(
            generator.tile_and_node_with_cancel(TileCoord::new(256, 0), &|| false),
            Ok(None)
        );
        assert_eq!(
            generator.edge_between_with_cancel(
                TileCoord::new(-1, 0),
                TileCoord::new(0, 0),
                &|| false
            ),
            Err(EnvironmentPageError::Invalid)
        );
        assert_eq!(
            generator.edge_between(TileCoord::new(-1, 0), TileCoord::new(0, 0)),
            EdgePassability::Blocked
        );
        // Adjacency rejection still precedes sampling/cancellation.
        assert_eq!(
            generator.edge_between_with_cancel(tile, tile, &|| true),
            Ok(EdgePassability::Blocked)
        );
    }
    for error in [
        EnvironmentPageError::Missing,
        EnvironmentPageError::Corrupt,
        EnvironmentPageError::Cancelled,
    ] {
        let generator = provider(Some(error));
        for _ in 0..2 {
            assert_eq!(
                generator.base_physical_tile_with_cancel(tile, &|| false),
                Err(error)
            );
            assert_eq!(
                generator.tile_and_node_with_cancel(tile, &|| false),
                Err(error)
            );
            assert_eq!(
                generator.edge_between_with_cancel(tile, next, &|| false),
                Err(error)
            );
            assert_eq!(generator.edge_between(tile, next), EdgePassability::Blocked);
        }
    }
}

#[derive(Debug)]
struct RecoveringProvider(std::sync::atomic::AtomicBool);

impl crate::EnvironmentPageProvider for RecoveringProvider {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if self.0.load(std::sync::atomic::Ordering::Relaxed) {
            Err(EnvironmentPageError::Missing)
        } else {
            Provider { error: None }.page(key, cancelled)
        }
    }
}

#[test]
fn provider_errors_are_not_cached_by_physical_or_combined_queries() {
    let provider = Arc::new(RecoveringProvider(std::sync::atomic::AtomicBool::new(true)));
    let mut generator = super::provider(None);
    generator.provider = Some(provider.clone());
    let tile = TileCoord::new(10, 10);
    let next = TileCoord::new(11, 10);
    assert_eq!(
        generator.edge_between_with_cancel(tile, next, &|| false),
        Err(EnvironmentPageError::Missing)
    );
    assert_eq!(
        generator.tile_and_node_with_cancel(tile, &|| false),
        Err(EnvironmentPageError::Missing)
    );
    provider
        .0
        .store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        generator.edge_between_with_cancel(tile, next, &|| false),
        Ok(EdgePassability::Passable)
    );
    assert!(
        generator
            .tile_and_node_with_cancel(tile, &|| false)
            .unwrap()
            .is_some()
    );
}

#[test]
fn legacy_pairs_and_edges_retain_the_old_ordered_queries() {
    for recipe in 3..=8 {
        let generator = flat(1, 16).with_elevation_sampling_recipe(recipe);
        for tile in [
            TileCoord::new(1, 1),
            TileCoord::new(8, 8),
            TileCoord::new(-1, 0),
        ] {
            let sample = generator.tile_at_with_cancel(tile, &|| false).unwrap();
            let node = generator.object_at_with_cancel(tile, &|| false).unwrap();
            assert_eq!(
                generator
                    .tile_and_node_with_cancel(tile, &|| false)
                    .unwrap(),
                sample.map(|sample| (sample, node))
            );
            let next = TileCoord::new(tile.x + 1, tile.y);
            if let (Some(from), Some(to)) = (sample, generator.tile_at(next)) {
                assert_eq!(generator.edge_between(tile, next), old_predicate(from, to));
            }
        }
    }
}
