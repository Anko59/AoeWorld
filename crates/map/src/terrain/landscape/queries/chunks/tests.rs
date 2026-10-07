use super::*;
use crate::biome::PreparedBiome;
use crate::terrain::elevation::PreparedElevation;
use crate::water::PreparedWater;
use crate::{CompactChunk, ElevationPage, PotentialBiomePage, WaterPage};
use std::sync::Arc;

fn fixture(width: i32, class: u8, height: i32) -> MapChunkGenerator {
    let mut generator =
        MapChunkGenerator::new([17; 32], 1, width).with_elevation_sampling_recipe(8);
    generator.elevation = Some(Arc::new(PreparedElevation {
        samples_per_axis: 1,
        compression: crate::Ratio::new(30, 1).expect("ratio"),
        sampling_recipe: 8,
        pages: [(
            (0, 0),
            ElevationPage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                geographic_height_centimeters: vec![height],
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
                potential_biome_class: vec![class],
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
fn policy(_: TileCoord, _: Tile) -> LandscapePolicy {
    LandscapePolicy {
        region: Region::Heavy,
        support_per_thousand: 1000,
    }
}
fn candidate(generator: &MapChunkGenerator, x: i32, y: i32) -> LandscapeChunk {
    generator
        .evaluate_landscape_chunk_with_cancel(x, y, &policy, &|_| Reservations::default(), &|| {
            false
        })
        .expect("source")
        .expect("bounded chunk")
}

#[test]
fn candidate_forest_families_floor_and_source_geometry_survive_compact_three() {
    for (class, family) in [
        (8, ResourceVisualFamily::Broadleaf),
        (14, ResourceVisualFamily::Conifer),
        (1, ResourceVisualFamily::Tropical),
        (4, ResourceVisualFamily::DryScrub),
    ] {
        let generator = fixture(128, class, 100);
        let published = generator.chunk(1, 1).expect("old chunk");
        let mut trees = 0;
        let mut floors = 0;
        for y in 0..4 {
            for x in 0..4 {
                let scene = candidate(&generator, x, y);
                assert_eq!(scene.tiles.len(), 1024);
                for tile in &scene.tiles {
                    let base = generator.sample_base_tile(tile.tile);
                    assert_eq!(
                        tile.terrain.geographic_height_centimeters,
                        base.geographic_height_centimeters
                    );
                    assert_eq!(tile.terrain.surface, base.surface);
                    assert_eq!(tile.terrain.passable, base.passable);
                    assert_eq!(
                        tile.terrain.vegetation_provenance,
                        base.vegetation_provenance
                    );
                    let appearance = tile.appearance.expect("candidate metadata");
                    assert_eq!(appearance.canopy_strength, appearance.floor_strength);
                    floors += usize::from(appearance.floor_strength > 0);
                    assert_eq!(
                        tile.terrain.material == GroundMaterial::ForestFloor,
                        appearance.floor_strength > 0
                    );
                }
                for resource in &scene.resources {
                    if resource.node.object == ObjectKind::Tree {
                        trees += 1;
                        assert_eq!(resource.visual_family, family);
                        assert_eq!(resource.node.kind, ResourceKind::Wood);
                    }
                }
                assert_eq!(
                    CompactChunk::encode_landscape(&scene)
                        .expect("encode")
                        .decode_landscape()
                        .expect("decode"),
                    scene
                );
            }
        }
        assert!(trees > 100 && floors >= trees);
        assert_eq!(generator.chunk(1, 1).expect("old chunk"), published);
    }
}

#[test]
fn shared_resource_approaches_and_routes_clear_floor_canopy_trees_and_dressing() {
    let generator = fixture(128, 8, 100);
    let reserved = |position: TileCoord| Reservations {
        route: position.x.rem_euclid(32) < 4,
        start: position.y.rem_euclid(32) < 4,
        ..Reservations::default()
    };
    let mut deposits = 0;
    for y in 0..4 {
        for x in 0..4 {
            let scene = generator
                .evaluate_landscape_chunk_with_cancel(x, y, &policy, &reserved, &|| false)
                .expect("source")
                .expect("bounded");
            for tile in &scene.tiles {
                let r = reserved(tile.tile);
                if r.route
                    || r.start
                    || generator
                        .landscape_resource_reserved(tile.tile, &|| false)
                        .expect("source")
                {
                    let appearance = tile.appearance.expect("metadata");
                    assert_eq!(appearance.floor_strength, 0);
                    assert_eq!(appearance.canopy_strength, 0);
                    assert!(
                        !scene
                            .resources
                            .iter()
                            .any(|node| node.node.tile == tile.tile
                                && node.node.object == ObjectKind::Tree)
                    );
                    assert!(!scene.decorations.iter().any(|node| node.tile == tile.tile));
                }
            }
            for resource in &scene.resources {
                assert!(!reserved(resource.node.tile).route && !reserved(resource.node.tile).start);
                if resource.node.object != ObjectKind::Tree {
                    deposits += 1;
                    assert!(
                        generator
                            .landscape_resource_at(
                                resource.node.tile,
                                generator.sample_base_tile(resource.node.tile),
                                &|| false
                            )
                            .expect("source")
                            .is_some()
                    );
                }
            }
        }
    }
    assert!(
        deposits > 0,
        "fixture must exercise non-tree resource reservations"
    );
}

#[test]
fn sparse_edge_chunks_and_shuffled_requests_are_coordinate_stable() {
    let generator = fixture(50, 8, 100);
    let scene = candidate(&generator, 1, 1);
    assert_eq!(scene.tiles.len(), 18 * 18);
    assert_eq!(scene.tiles[18].tile, TileCoord::new(32, 33));
    assert_eq!(
        CompactChunk::encode_landscape(&scene)
            .expect("sparse encode")
            .decode_landscape()
            .expect("decode"),
        scene
    );
    for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1), (0, 0)] {
        let first = candidate(&generator, x, y);
        let _ = candidate(&generator, 1, 1);
        assert_eq!(candidate(&generator, x, y), first);
    }
    assert_eq!(
        generator
            .evaluate_landscape_chunk_with_cancel(
                2,
                0,
                &policy,
                &|_| Reservations::default(),
                &|| false
            )
            .expect("bounds"),
        None
    );
    assert_eq!(
        generator.evaluate_landscape_chunk_with_cancel(
            i32::MAX,
            0,
            &policy,
            &|_| Reservations::default(),
            &|| false
        ),
        Err(EnvironmentPageError::Invalid)
    );
    assert_eq!(
        generator.evaluate_landscape_chunk_with_cancel(
            0,
            0,
            &policy,
            &|_| Reservations::default(),
            &|| true
        ),
        Err(EnvironmentPageError::Cancelled)
    );
}

#[test]
fn temperate_summer_treeline_and_savanna_never_get_dense_forest_dressing() {
    for (class, height, expected) in [
        (8, 240_000, NativeHeightBand::Subalpine),
        (14, 300_000, NativeHeightBand::Alpine),
        (14, 400_000, NativeHeightBand::Nival),
        (20, 100, NativeHeightBand::Lowland),
    ] {
        let generator = fixture(64, class, height);
        let scene = candidate(&generator, 0, 0);
        for tile in &scene.tiles {
            let appearance = tile.appearance.expect("metadata");
            assert_eq!(appearance.height_band, expected);
            assert_eq!(appearance.floor_strength, 0);
        }
        assert!(
            !scene
                .resources
                .iter()
                .any(|resource| resource.node.object == ObjectKind::Tree)
        );
        if expected == NativeHeightBand::Alpine {
            assert!(
                scene
                    .tiles
                    .iter()
                    .all(|t| t.terrain.material == GroundMaterial::Rock)
            );
        }
        if expected == NativeHeightBand::Nival {
            assert!(
                scene
                    .tiles
                    .iter()
                    .all(|t| t.terrain.material == GroundMaterial::Snow)
            );
        }
    }
    let generator = fixture(128, 16, 100);
    let mut trees = 0;
    for y in 0..4 {
        for x in 0..4 {
            let scene = candidate(&generator, x, y);
            assert!(
                scene
                    .tiles
                    .iter()
                    .all(|tile| tile.appearance.expect("metadata").floor_strength == 0)
            );
            trees += scene
                .resources
                .iter()
                .filter(|r| r.node.object == ObjectKind::Tree)
                .count();
            assert_eq!(
                CompactChunk::encode_landscape(&scene)
                    .expect("encode")
                    .decode_landscape()
                    .expect("decode"),
                scene
            );
        }
    }
    assert!(trees > 0 && trees < 1640);
}

#[test]
fn historical_clearings_cannot_be_refilled_by_decorations() {
    let mut generator = fixture(64, 8, 100);
    let page = crate::HistoricalLandUsePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        crop_percent: vec![100],
        grazing_percent: vec![0],
        population_pressure_per_square_kilometer: vec![0],
        coverage: vec![crate::HistoricalCoverage {
            land_percent: 100,
            valid_land_percent: 100,
            ..crate::HistoricalCoverage::default()
        }],
    };
    page.validate().expect("valid source");
    generator.historical_land_use = Some(Arc::new(crate::land_use::HistoricalLandUse::new(
        1,
        [((0, 0), page)].into(),
    )));
    let scene = candidate(&generator, 0, 0);
    assert!(scene.decorations.is_empty());
    assert!(
        scene
            .tiles
            .iter()
            .all(|tile| tile.appearance.expect("metadata").floor_strength == 0)
    );
    assert!(
        !scene
            .resources
            .iter()
            .any(|resource| resource.node.object == ObjectKind::Tree)
    );
}
