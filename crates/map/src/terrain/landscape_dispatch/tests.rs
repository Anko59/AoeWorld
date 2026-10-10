use super::*;
use crate::biome::PreparedBiome;
use crate::water::PreparedWater;
use crate::{
    CompactChunk, ElevationPage, EnvironmentPage, EnvironmentPageKey, FieldPyramid,
    PotentialBiomePage, PreparedEnvironment, PyramidLevel, Ratio, WaterPage,
};
use std::sync::Arc;

#[path = "tests/physical_queries.rs"]
mod physical_queries;
#[path = "tests/route_probe.rs"]
mod route_probe;
#[path = "tests/source_families.rs"]
mod source_families;

const SEEDS: [u64; 16] = [0, 1, 2, 3, 7, 11, 17, 18, 23, 29, 31, 47, 63, 71, 97, 255];
fn flat(seed: u64, width: i32) -> MapChunkGenerator {
    let mut generator = MapChunkGenerator::new([17; 32], seed, width);
    generator.elevation = Some(Arc::new(PreparedElevation {
        samples_per_axis: 1,
        compression: Ratio::new(30, 1).unwrap(),
        pages: [(
            (0, 0),
            ElevationPage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                geographic_height_centimeters: vec![100],
            },
        )]
        .into(),
    }));
    generator.biome = Some(Arc::new(PreparedBiome::new(
        1,
        [(
            (0, 0),
            PotentialBiomePage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                potential_biome_class: vec![14],
            },
        )]
        .into(),
    )));
    generator.water = Some(Arc::new(PreparedWater::new(
        1,
        [(
            (0, 0),
            WaterPage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                ocean_coverage_percent: vec![0],
                inland_coverage_percent: vec![0],
            },
        )]
        .into(),
    )));
    generator
}

#[test]
fn all_sixteen_seeds_authoritative_points_objects_blocking_and_sparse_chunks_agree() {
    let mut total_trees = 0;
    for seed in SEEDS {
        for (width, x, y) in [(256, 0, 0), (50, 1, 1)] {
            let generator = flat(seed, width);
            let scene = generator
                .landscape_chunk_with_cancel(x, y, &|| false)
                .unwrap();
            let projection = generator.chunk_with_cancel(x, y, &|| false).unwrap();
            assert_eq!(
                projection.tiles,
                scene
                    .tiles
                    .iter()
                    .map(|tile| tile.terrain)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                projection.resources,
                scene
                    .resources
                    .iter()
                    .map(|resource| resource.node)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                CompactChunk::encode(&scene).unwrap().decode().unwrap(),
                scene
            );
            if width == 50 {
                assert_eq!(scene.tiles.len(), 18 * 18);
                assert_eq!(scene.tiles[18].tile, TileCoord::new(32, 33));
            }
            for tile in &scene.tiles {
                let point = generator
                    .landscape_point_with_cancel(tile.tile, &|| false)
                    .unwrap()
                    .unwrap();
                assert_eq!(point.tile, *tile);
                assert_eq!(
                    generator.tile_at_with_cancel(tile.tile, &|| false).unwrap(),
                    Some(tile.terrain)
                );
                assert_eq!(generator.tile_at(tile.tile), Some(tile.terrain));
                let resource = scene
                    .resources
                    .iter()
                    .find(|r| r.node.tile == tile.tile)
                    .copied();
                assert_eq!(point.resource, resource);
                let node = resource.map(|resource| resource.node);
                assert_eq!(
                    generator
                        .object_at_with_cancel(tile.tile, &|| false)
                        .unwrap(),
                    node
                );
                assert_eq!(generator.object_at(tile.tile), node);
                assert_eq!(
                    point.decoration,
                    scene
                        .decorations
                        .iter()
                        .find(|d| d.tile == tile.tile)
                        .copied()
                );
                if let Some(node) = node {
                    assert_eq!(
                        generator
                            .resource_by_id_with_cancel(node.id, &|| false)
                            .unwrap(),
                        Some(node)
                    );
                    total_trees += usize::from(node.object == ObjectKind::Tree);
                    if node.object != ObjectKind::Tree {
                        let mut has_access = false;
                        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                            if let Some(neighbor) = generator
                                .landscape_point_with_cancel(
                                    TileCoord::new(node.tile.x + dx, node.tile.y + dy),
                                    &|| false,
                                )
                                .unwrap()
                            {
                                has_access |= neighbor.tile.terrain.passable
                                    && neighbor.resource.is_none()
                                    && neighbor.decoration.is_none()
                                    && neighbor.tile.appearance.floor_strength == 0
                                    && (i32::from(tile.terrain.game_height_level)
                                        - i32::from(neighbor.tile.terrain.game_height_level))
                                    .abs()
                                        <= 1;
                            }
                        }
                        assert!(
                            has_access,
                            "each placed non-tree resource needs a clear physical cardinal approach"
                        );
                    }
                }
            }
        }
    }
    assert!(
        total_trees > 100,
        "fixture must exercise dense forests, not only cleared starts"
    );
}

#[test]
fn starts_routes_history_and_resource_approaches_share_the_same_forest_mask() {
    let generator = flat(1, 256);
    let scene = generator
        .landscape_chunk_with_cancel(3, 3, &|| false)
        .unwrap();
    let mut starts = 0;
    for tile in &scene.tiles {
        let reservation = generator.landscape_reservations_at(tile.tile);
        if reservation.start || reservation.route {
            starts += 1;
            let appearance = tile.appearance;
            assert_eq!(
                (appearance.canopy_strength, appearance.floor_strength),
                (0, 0)
            );
            assert!(
                generator
                    .object_at_with_cancel(tile.tile, &|| false)
                    .unwrap()
                    .is_none()
            );
            assert!(!scene.decorations.iter().any(|d| d.tile == tile.tile));
        }
    }
    assert!(starts > 100);
    let mut cleared = generator;
    cleared.historical_land_use = Some(Arc::new(crate::land_use::HistoricalLandUse::new(
        1,
        [(
            (0, 0),
            crate::HistoricalLandUsePage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                crop_percent: vec![100],
                grazing_percent: vec![0],
                population_pressure_per_square_kilometer: vec![0],
                coverage: Vec::new(),
            },
        )]
        .into(),
    )));
    let scene = cleared
        .landscape_chunk_with_cancel(0, 0, &|| false)
        .unwrap();
    assert!(scene.tiles.iter().all(|t| t.appearance.floor_strength == 0));
    assert!(
        scene
            .resources
            .iter()
            .all(|r| r.node.object != ObjectKind::Tree)
    );
    assert!(scene.decorations.is_empty());
}

#[test]
fn seeded_start_connections_clear_full_neighbor_segments_without_changing_source_geometry() {
    let generator = flat(1, 512);
    let origin = TileCoord::new(255, 255);
    let repeated = flat(1, 512);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let target = generator
                .forest_opening_center_at(TileCoord::new(origin.x + dx * 192, origin.y + dy * 192))
                .unwrap();
            let steps = origin
                .x
                .abs_diff(target.x)
                .max(origin.y.abs_diff(target.y))
                .max(1);
            for step in 0..=steps {
                let position = TileCoord::new(
                    origin.x
                        + ((i64::from(target.x - origin.x) * i64::from(step)) / i64::from(steps))
                            as i32,
                    origin.y
                        + ((i64::from(target.y - origin.y) * i64::from(step)) / i64::from(steps))
                            as i32,
                );
                let reserved = generator.landscape_reservations_at(position);
                assert!(
                    reserved.route,
                    "entire seeded start-node segment must be reserved"
                );
                assert_eq!(reserved, repeated.landscape_reservations_at(position));
                if step % 32 == 0 || step == steps {
                    let point = generator
                        .landscape_point_with_cancel(position, &|| false)
                        .unwrap()
                        .unwrap();
                    let appearance = point.tile.appearance;
                    assert_eq!(
                        (appearance.canopy_strength, appearance.floor_strength),
                        (0, 0)
                    );
                    assert!(point.resource.is_none());
                    assert!(point.decoration.is_none());
                    let source = generator.sample_base_tile(position);
                    assert_eq!(
                        point.tile.terrain.geographic_height_centimeters,
                        source.geographic_height_centimeters
                    );
                    assert_eq!(point.tile.terrain.surface, source.surface);
                    assert_eq!(point.tile.terrain.passable, source.passable);
                }
            }
        }
    }
}

#[derive(Debug)]
struct Provider {
    error: Option<EnvironmentPageError>,
}
impl crate::EnvironmentPageProvider for Provider {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        assert_eq!((key.level, key.x, key.y), (0, 0, 0));
        Ok(Arc::new(match key.layer {
            crate::PageLayer::Elevation => EnvironmentPage::Elevation(ElevationPage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                geographic_height_centimeters: vec![100],
            }),
            crate::PageLayer::Vegetation => EnvironmentPage::Vegetation(PotentialBiomePage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                potential_biome_class: vec![14],
            }),
            crate::PageLayer::Water => EnvironmentPage::Water(WaterPage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                ocean_coverage_percent: vec![0],
                inland_coverage_percent: vec![0],
            }),
            _ => panic!("unexpected source layer"),
        }))
    }
}
fn provider(error: Option<EnvironmentPageError>) -> MapChunkGenerator {
    let field = FieldPyramid {
        levels: vec![PyramidLevel {
            samples_per_axis: 1,
            ordered_page_root: [1; 32],
        }],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        elevation: field.clone(),
        vegetation: Some(field.clone()),
        water: Some(field),
        ..PreparedEnvironment::default()
    };
    let mut generator = flat(1, 256);
    generator.provider = Some(Arc::new(Provider { error }));
    generator.provider_environment = Some(Arc::new(environment));
    generator.provider_compression = Some(Ratio::new(30, 1).unwrap());
    generator
}
#[test]
fn source_provider_and_dense_dispatch_match_and_page_failures_never_fallback() {
    let dense = flat(1, 256);
    let source = provider(None);
    assert_eq!(
        source.landscape_chunk_with_cancel(0, 0, &|| false).unwrap(),
        dense.landscape_chunk_with_cancel(0, 0, &|| false).unwrap()
    );
    for error in [
        EnvironmentPageError::Missing,
        EnvironmentPageError::Corrupt,
        EnvironmentPageError::Cancelled,
    ] {
        let source = provider(Some(error));
        let tile = TileCoord::new(10, 10);
        assert_eq!(source.tile_at_with_cancel(tile, &|| false), Err(error));
        assert_eq!(source.object_at_with_cancel(tile, &|| false), Err(error));
        assert_eq!(source.chunk_with_cancel(0, 0, &|| false), Err(error));
        assert_eq!(
            source.landscape_chunk_with_cancel(0, 0, &|| false),
            Err(error)
        );
    }
    let source = provider(None);
    let tile = TileCoord::new(10, 10);
    assert_eq!(
        source.landscape_point_with_cancel(tile, &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
    assert_eq!(
        source.object_at_with_cancel(tile, &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
    assert_eq!(
        source.landscape_chunk_with_cancel(i32::MAX, 0, &|| false),
        Err(EnvironmentPageError::Invalid)
    );
    assert!(
        source
            .landscape_point_with_cancel(TileCoord::new(-1, 0), &|| false)
            .unwrap()
            .is_none()
    );
}
