use super::*;
use crate::biome::PreparedBiome;
use crate::terrain::elevation::PreparedElevation;
use crate::water::PreparedWater;
use crate::{CompactChunk, ElevationPage, PotentialBiomePage, WaterPage};
use std::sync::Arc;

fn fixture(width: i32, class: u8, height: i32) -> MapChunkGenerator {
    let mut generator = MapChunkGenerator::new([17; 32], 1, width);
    generator.elevation = Some(Arc::new(PreparedElevation {
        samples_per_axis: 1,
        compression: crate::Ratio::new(30, 1).expect("ratio"),
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
        wildness_per_thousand: Some(1000),
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
fn candidate_forest_families_floor_and_source_geometry_survive_the_codec() {
    for (class, family) in [
        (8, ResourceVisualFamily::Conifer),
        (13, ResourceVisualFamily::Broadleaf),
        (14, ResourceVisualFamily::Conifer),
        (1, ResourceVisualFamily::Tropical),
        (4, ResourceVisualFamily::DryScrub),
    ] {
        let generator = fixture(128, class, 100);
        let published = generator.chunk(1, 1).expect("default chunk");
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
                    let appearance = tile.appearance;
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
                    CompactChunk::encode(&scene)
                        .expect("encode")
                        .decode()
                        .expect("decode"),
                    scene
                );
            }
        }
        assert!(trees > 100 && floors >= trees);
        assert_eq!(generator.chunk(1, 1).expect("default chunk"), published);
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
    let (mut deposits, mut corridors) = (0, 0);
    for y in 0..4 {
        for x in 0..4 {
            let scene = generator
                .evaluate_landscape_chunk_with_cancel(x, y, &policy, &reserved, &|| false)
                .expect("source")
                .expect("bounded");
            for tile in &scene.tiles {
                let r = reserved(tile.tile);
                // A route is a corridor: it clears only inside a large forest.
                let corridor = r.route
                    && generator
                        .evaluate_landscape_with_cancel(
                            tile.tile,
                            policy(tile.tile, tile.terrain),
                            &reserved,
                            &|| false,
                        )
                        .expect("source")
                        .expect("bounded")
                        .density
                        .opening
                        != crate::landscape_patches::Opening::None;
                corridors += usize::from(corridor);
                if corridor
                    || r.start
                    || generator
                        .landscape_resource_reserved(tile.tile, &|| false)
                        .expect("source")
                {
                    let appearance = tile.appearance;
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
                let r = reserved(resource.node.tile);
                assert!(!r.start);
                if resource.node.object != ObjectKind::Tree {
                    assert!(!r.route);
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
    assert!(
        corridors > 100,
        "fixture must exercise large-forest corridors"
    );
}

#[test]
fn sparse_edge_chunks_and_shuffled_requests_are_coordinate_stable() {
    let generator = fixture(50, 8, 100);
    let scene = candidate(&generator, 1, 1);
    assert_eq!(scene.tiles.len(), 18 * 18);
    assert_eq!(scene.tiles[18].tile, TileCoord::new(32, 33));
    assert_eq!(
        CompactChunk::encode(&scene)
            .expect("sparse encode")
            .decode()
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
            let appearance = tile.appearance;
            assert_eq!(appearance.height_band, expected);
            assert_eq!(appearance.floor_strength, 0);
        }
        assert!(
            !scene
                .resources
                .iter()
                .any(|resource| resource.node.object == ObjectKind::Tree)
        );
        // At the default latitude the snow line wanders around 2,770 m and
        // the rock line around 2,230 m.
        let expected_ground: &[GroundMaterial] = match expected {
            NativeHeightBand::Nival => &[GroundMaterial::Snow],
            NativeHeightBand::Alpine => &[
                GroundMaterial::Snow,
                GroundMaterial::Rock,
                GroundMaterial::Gravel,
                GroundMaterial::Dirt,
            ],
            NativeHeightBand::Subalpine => &[
                GroundMaterial::Snow,
                GroundMaterial::Rock,
                GroundMaterial::TemperateGrass,
                GroundMaterial::WornGrass,
                GroundMaterial::GrassDirt,
                GroundMaterial::LushGrass,
                GroundMaterial::Dirt,
                GroundMaterial::Gravel,
            ],
            _ => &[
                GroundMaterial::DryGrass,
                GroundMaterial::WornGrass,
                GroundMaterial::GrassDirt,
                GroundMaterial::Dirt,
            ],
        };
        assert!(
            scene
                .tiles
                .iter()
                .all(|tile| expected_ground.contains(&tile.terrain.material))
        );
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
                    .all(|tile| tile.appearance.floor_strength == 0)
            );
            trees += scene
                .resources
                .iter()
                .filter(|r| r.node.object == ObjectKind::Tree)
                .count();
            assert_eq!(
                CompactChunk::encode(&scene)
                    .expect("encode")
                    .decode()
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
    // Fully cropped country has no wildness left: the source signal, not a
    // parcel mask, keeps it free of forest.
    let scene = generator
        .landscape_chunk_with_cancel(0, 0, &|| false)
        .expect("source");
    assert!(scene.decorations.is_empty());
    assert!(
        scene
            .tiles
            .iter()
            .all(|tile| tile.appearance.floor_strength == 0)
    );
    assert!(
        !scene
            .resources
            .iter()
            .any(|resource| resource.node.object == ObjectKind::Tree)
    );
}

#[test]
fn ground_variety_changes_only_the_drawn_sheet_and_stays_in_its_family() {
    use GroundMaterial::{Dirt, DryGrass, GrassDirt, Gravel, Sand, WornGrass};
    let families: [(u8, &[GroundMaterial]); 2] = [
        (20, &[DryGrass, GrassDirt, Dirt, WornGrass]),
        (27, &[Sand, Gravel, Dirt]),
    ];
    for (class, family) in families {
        let generator = fixture(512, class, 20_000);
        let mut seen = Vec::new();
        for (x, y) in [(0, 0), (3, 5), (7, 7)] {
            let chunk = candidate(&generator, x, y);
            assert_eq!(chunk.tiles.len(), 1024);
            for tile in &chunk.tiles {
                let base = generator
                    .landscape_base_at(tile.tile, &|| false)
                    .expect("source")
                    .expect("in bounds");
                // Heights, surface, water, biome, provenance and passability
                // are the base tile's: variety is not a physical input.
                let mut physical = tile.terrain;
                physical.material = base.material;
                assert_eq!(physical, base);
                assert!(family.contains(&tile.terrain.material), "class {class}");
                if !seen.contains(&tile.terrain.material) {
                    seen.push(tile.terrain.material);
                }
                // Seamless: a single point query draws the same sheet.
                if tile.tile.x % 32 == 31 || tile.tile.y % 32 == 0 {
                    let point = generator
                        .evaluate_landscape_point_with_cancel(
                            tile.tile,
                            &policy,
                            &|_| Reservations::default(),
                            &|| false,
                        )
                        .expect("source")
                        .expect("in bounds");
                    assert_eq!(point.tile, *tile);
                }
            }
        }
        assert!(seen.len() >= 3, "class {class} shows {seen:?}");
    }
}
