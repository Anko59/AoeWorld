use super::*;

fn generator(seed: u64) -> MapChunkGenerator {
    MapChunkGenerator::new([3; 32], seed, 128)
}

#[test]
fn chunks_are_order_independent_and_have_stable_shared_tiles() {
    let first = generator(1).chunk(0, 0);
    let second = generator(1).chunk(0, 0);
    assert_eq!(first, second);
    assert_eq!(
        generator(1).tile_at(TileCoord::new(31, 5)),
        generator(1).chunk(0, 0).tiles.get(5 * 32 + 31).copied()
    );
}

#[test]
fn procedural_seed_changes_objects_without_moving_relief() {
    let tile = TileCoord::new(13, 8);
    assert_eq!(
        generator(1)
            .tile_at(tile)
            .expect("tile")
            .geographic_height_centimeters,
        generator(2)
            .tile_at(tile)
            .expect("tile")
            .geographic_height_centimeters
    );
    let first = (0..4)
        .flat_map(|y| (0..4).flat_map(move |x| generator(1).chunk(x, y).resources))
        .collect::<Vec<_>>();
    let second = (0..4)
        .flat_map(|y| (0..4).flat_map(move |x| generator(2).chunk(x, y).resources))
        .collect::<Vec<_>>();
    assert_ne!(first, second);
}

#[test]
fn resources_have_one_collision_free_slot_per_tile() {
    let chunk = generator(1).chunk(0, 0);
    let mut ids = chunk
        .resources
        .iter()
        .map(|node| node.id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), chunk.resources.len());
}

#[test]
fn historical_land_use_clears_wood_without_creating_settlements() {
    let level_zero = HistoricalLandUsePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        crop_percent: vec![100; 4],
        grazing_percent: vec![0; 4],
        population_pressure_per_square_kilometer: vec![u16::MAX; 4],
    };
    let overview = HistoricalLandUsePage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        crop_percent: vec![100],
        grazing_percent: vec![0],
        population_pressure_per_square_kilometer: vec![u16::MAX],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: crate::FieldPyramid {
            levels: vec![
                crate::PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: [1; 32],
                },
                crate::PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: [2; 32],
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: Some(crate::FieldPyramid {
            levels: vec![
                crate::PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: crate::ordered_land_use_page_root(std::slice::from_ref(
                        &level_zero,
                    ))
                    .expect("level-zero root"),
                },
                crate::PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: crate::ordered_land_use_page_root(std::slice::from_ref(
                        &overview,
                    ))
                    .expect("overview root"),
                },
            ],
        }),
    };
    let natural = (0..4)
        .flat_map(|y| (0..4).flat_map(move |x| generator(1).chunk(x, y).resources))
        .collect::<Vec<_>>();
    assert!(natural.iter().any(|node| node.kind == ResourceKind::Wood));
    let cleared = generator(1)
        .with_historical_land_use(&environment, vec![level_zero, overview])
        .expect("prepared land use");
    let cleared = (0..4)
        .flat_map(|y| {
            let generator = cleared.clone();
            (0..4).flat_map(move |x| generator.chunk(x, y).resources)
        })
        .collect::<Vec<_>>();
    assert!(cleared.iter().all(|node| node.kind != ResourceKind::Wood));
}

#[test]
fn prepared_inland_coverage_creates_a_non_passable_lake() {
    let level_zero = crate::WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        ocean_coverage_percent: vec![0; 4],
        inland_coverage_percent: vec![100, 0, 0, 0],
    };
    let overview = crate::WaterPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![0],
        inland_coverage_percent: vec![25],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: crate::FieldPyramid {
            levels: vec![
                crate::PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: [1; 32],
                },
                crate::PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: [2; 32],
                },
            ],
        },
        water: Some(crate::FieldPyramid {
            levels: vec![
                crate::PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: crate::ordered_water_page_root(std::slice::from_ref(
                        &level_zero,
                    ))
                    .expect("level-zero root"),
                },
                crate::PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: crate::ordered_water_page_root(std::slice::from_ref(
                        &overview,
                    ))
                    .expect("overview root"),
                },
            ],
        }),
        vegetation: None,
        historical_land_use: None,
    };
    let terrain = generator(1)
        .with_prepared_water(&environment, vec![level_zero, overview])
        .expect("prepared water");
    let tile = terrain.tile_at(TileCoord::new(0, 0)).expect("lake tile");
    assert_eq!(tile.water, WaterKind::Lake);
    assert_eq!(tile.water_provenance, Provenance::SourceDerived);
    assert!(!tile.passable);
}
