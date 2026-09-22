use super::*;

mod elevation_interpolation;

fn generator(seed: u64) -> MapChunkGenerator {
    MapChunkGenerator::new([3; 32], seed, 128)
}

fn flat_generator(geography_key: [u8; 32], procedural_seed: u64) -> MapChunkGenerator {
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![0],
        inland_coverage_percent: vec![0],
    };
    let biome = PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        potential_biome_class: vec![16],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: crate::FieldPyramid {
            levels: vec![crate::PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: crate::ordered_page_root(std::slice::from_ref(&elevation))
                    .expect("elevation root"),
            }],
        },
        water: Some(crate::FieldPyramid {
            levels: vec![crate::PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: crate::ordered_water_page_root(std::slice::from_ref(&water))
                    .expect("water root"),
            }],
        }),
        vegetation: Some(crate::FieldPyramid {
            levels: vec![crate::PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: crate::ordered_biome_page_root(std::slice::from_ref(&biome))
                    .expect("biome root"),
            }],
        }),
        historical_land_use: None,
    };
    MapChunkGenerator::new(geography_key, procedural_seed, 512)
        .with_prepared_elevation(
            Ratio {
                numerator: 1,
                denominator: 1,
            },
            &environment,
            vec![elevation],
        )
        .expect("flat elevation")
        .with_prepared_water(&environment, vec![water])
        .expect("flat water")
        .with_prepared_biomes(&environment, vec![biome])
        .expect("flat biome")
}

#[test]
fn fixed_flat_patch_rejects_raw_resource_blockers_and_accepts_open_neighbor() {
    let terrain = flat_generator([11; 32], 0);
    let detail_key = super::resources::detail_key(&terrain);
    let (root, value, count) =
        super::resources::patch_origin(detail_key, b"gold-patch-v2", 0, 0, 192, 96);
    assert_eq!(count, 7);
    let center = terrain.tile_at(root).expect("flat patch center");
    assert!(super::resources::candidate(&terrain, root, center).is_some());
    let neighbors = [
        TileCoord::new(root.x - 1, root.y),
        TileCoord::new(root.x + 1, root.y),
        TileCoord::new(root.x, root.y - 1),
        TileCoord::new(root.x, root.y + 1),
    ];
    assert!(neighbors.iter().all(|neighbor| {
        terrain
            .tile_at(*neighbor)
            .and_then(|sample| super::resources::candidate(&terrain, *neighbor, sample))
            .is_some()
    }));
    assert!(
        neighbors
            .iter()
            .any(|neighbor| terrain.object_at(*neighbor).is_some())
    );
    assert!(terrain.object_at(root).is_none());
    let open = TileCoord::new(root.x + 1, root.y + 1);
    let open_sample = terrain.tile_at(open).expect("open patch tile");
    assert!(super::resources::candidate(&terrain, open, open_sample).is_some());
    assert!(
        terrain
            .object_at(open)
            .is_some_and(|node| node.kind == ResourceKind::Gold)
    );
    assert_ne!(value, 0);
}

#[test]
fn deterministic_tree_is_an_obstruction_for_gathering_access() {
    let terrain = flat_generator([11; 32], 0);
    let tree = TileCoord::new(23, 0);
    let sample = terrain.tile_at(tree).expect("tree tile");
    assert_eq!(
        terrain.object_at(tree).map(|node| node.object),
        Some(ObjectKind::Tree)
    );
    assert!(terrain.occupied_without_access(tree, sample));
}

#[test]
fn chunks_are_order_independent_and_have_stable_shared_tiles() {
    let first = generator(1).chunk(0, 0).expect("fixture chunk");
    let second = generator(1).chunk(0, 0).expect("fixture chunk");
    assert_eq!(first, second);
    assert_eq!(
        generator(1).tile_at(TileCoord::new(31, 5)),
        generator(1)
            .chunk(0, 0)
            .expect("fixture chunk")
            .tiles
            .get(5 * 32 + 31)
            .copied()
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
        .flat_map(|y| {
            (0..4).flat_map(move |x| generator(1).chunk(x, y).expect("fixture chunk").resources)
        })
        .collect::<Vec<_>>();
    let second = (0..4)
        .flat_map(|y| {
            (0..4).flat_map(move |x| generator(2).chunk(x, y).expect("fixture chunk").resources)
        })
        .collect::<Vec<_>>();
    assert_ne!(first, second);
}

#[test]
fn resources_have_one_collision_free_slot_per_tile() {
    let chunk = generator(1).chunk(0, 0).expect("fixture chunk");
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
fn resources_are_stable_when_chunks_cross_boundaries_in_different_orders() {
    let terrain = generator(17);
    let coordinates = [(0, 0), (1, 0), (0, 1), (1, 1)];
    let mut forward = coordinates
        .into_iter()
        .flat_map(|(x, y)| terrain.chunk(x, y).expect("fixture chunk").resources)
        .collect::<Vec<_>>();
    let mut reverse = coordinates
        .into_iter()
        .rev()
        .flat_map(|(x, y)| terrain.chunk(x, y).expect("fixture chunk").resources)
        .collect::<Vec<_>>();
    forward.sort_by_key(|node| node.id);
    reverse.sort_by_key(|node| node.id);
    assert_eq!(forward, reverse);
}

#[test]
fn every_resource_has_an_unoccupied_adjacent_gathering_tile() {
    let terrain = generator(23);
    let mut nodes = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            nodes.extend(terrain.chunk(x, y).expect("fixture chunk").resources);
        }
    }
    let mut examined = 0;
    for node in nodes
        .into_iter()
        .filter(|node| node.kind != ResourceKind::Wood)
    {
        examined += 1;
        let neighbors = [
            TileCoord::new(node.tile.x - 1, node.tile.y),
            TileCoord::new(node.tile.x + 1, node.tile.y),
            TileCoord::new(node.tile.x, node.tile.y - 1),
            TileCoord::new(node.tile.x, node.tile.y + 1),
        ];
        assert!(neighbors.into_iter().any(|neighbor| {
            terrain
                .tile_at(neighbor)
                .is_some_and(|sample| sample.passable && terrain.object_at(neighbor).is_none())
        }));
    }
    assert!(examined > 0);
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
        .flat_map(|y| {
            (0..4).flat_map(move |x| generator(1).chunk(x, y).expect("fixture chunk").resources)
        })
        .collect::<Vec<_>>();
    assert!(natural.iter().any(|node| node.kind == ResourceKind::Wood));
    let cleared = generator(1)
        .with_historical_land_use(&environment, vec![level_zero, overview])
        .expect("prepared land use");
    let cleared = (0..4)
        .flat_map(|y| {
            let generator = cleared.clone();
            (0..4).flat_map(move |x| generator.chunk(x, y).expect("fixture chunk").resources)
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
    let dry = terrain
        .tile_at(TileCoord::new(127, 127))
        .expect("dry coverage tile");
    assert_eq!(dry.water, WaterKind::None);
    assert_eq!(dry.water_provenance, Provenance::SourceDerived);
    assert!(dry.passable);
}
