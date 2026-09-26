use super::clearing::{CELL_TILES, CORE_TILES, Geometry};
use super::*;
use crate::{
    EnvironmentPage, EnvironmentPageError, EnvironmentPageKey, EnvironmentPageProvider,
    FieldPyramid, HistoricalLandUsePage, PyramidLevel,
};
use aoe_core::TileCoord;
use std::{collections::BTreeMap, sync::Arc};

const PARITY_RECIPES: [u16; 3] = [
    crate::LEGACY_GENERATION_RECIPE_VERSION,
    crate::PRIOR_GENERATION_RECIPE_VERSION,
    crate::GENERATION_RECIPE_VERSION,
];
const SAMPLED_KEYS: [[u8; 32]; 16] = [
    [0; 32], [1; 32], [2; 32], [3; 32], [7; 32], [11; 32], [17; 32], [18; 32], [23; 32], [29; 32],
    [31; 32], [47; 32], [63; 32], [71; 32], [97; 32], [255; 32],
];

#[derive(Debug)]
struct Pages(BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>);

impl EnvironmentPageProvider for Pages {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        self.0
            .get(&key)
            .cloned()
            .ok_or(EnvironmentPageError::Missing)
    }
}

fn recipe_generator(key: [u8; 32], seed: u64, recipe: u16) -> MapChunkGenerator {
    MapChunkGenerator::new(key, seed, 512).with_elevation_sampling_recipe(recipe)
}

fn constant_pages() -> (
    PreparedEnvironment,
    Vec<ElevationPage>,
    Vec<WaterPage>,
    Vec<PotentialBiomePage>,
    Pages,
) {
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
        potential_biome_class: vec![1],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: crate::ordered_page_root(std::slice::from_ref(&elevation))
                    .expect("elevation root"),
            }],
        },
        water: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: crate::ordered_water_page_root(std::slice::from_ref(&water))
                    .expect("water root"),
            }],
        }),
        vegetation: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: crate::ordered_biome_page_root(std::slice::from_ref(&biome))
                    .expect("biome root"),
            }],
        }),
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let mut index = BTreeMap::new();
    for page in [
        EnvironmentPage::Elevation(elevation.clone()),
        EnvironmentPage::Water(water.clone()),
        EnvironmentPage::Vegetation(biome.clone()),
    ] {
        let key = page.key();
        index.insert(key, Arc::new(page));
    }
    (
        environment,
        vec![elevation],
        vec![water],
        vec![biome],
        Pages(index),
    )
}

fn prepared_generator(
    key: [u8; 32],
    seed: u64,
    recipe: u16,
    environment: &PreparedEnvironment,
    elevation: Vec<ElevationPage>,
    water: Vec<WaterPage>,
    biome: Vec<PotentialBiomePage>,
) -> MapChunkGenerator {
    MapChunkGenerator::new(key, seed, 288)
        .with_elevation_sampling_recipe(recipe)
        .with_prepared_elevation(Ratio::new(1, 1).expect("ratio"), environment, elevation)
        .expect("dense elevation")
        .with_prepared_water(environment, water)
        .expect("dense water")
        .with_prepared_biomes(environment, biome)
        .expect("dense biome")
}

fn provider_generator(
    key: [u8; 32],
    seed: u64,
    recipe: u16,
    environment: PreparedEnvironment,
    pages: Pages,
) -> MapChunkGenerator {
    MapChunkGenerator::new(key, seed, 288)
        .with_elevation_sampling_recipe(recipe)
        .with_page_provider(
            Ratio::new(1, 1).expect("ratio"),
            environment,
            Arc::new(pages),
        )
        .expect("provider")
}

#[test]
fn clearing_hash_is_deterministic_per_geography_and_independent_of_object_seed() {
    let tile = TileCoord::new(143, 211);
    let first = recipe_generator([17; 32], 9, crate::GENERATION_RECIPE_VERSION);
    let repeated = recipe_generator([17; 32], 99, crate::GENERATION_RECIPE_VERSION);
    let other_geography = recipe_generator([18; 32], 9, crate::GENERATION_RECIPE_VERSION);
    let first_shape = clearing::geometry(&first, tile).expect("recipe-five clearing");
    assert_eq!(
        first_shape,
        clearing::geometry(&repeated, tile).expect("repeat")
    );
    assert_ne!(
        first_shape,
        clearing::geometry(&other_geography, tile).expect("other geography")
    );
    let cell_center = TileCoord::new(
        first_shape.cell.0 * CELL_TILES + CELL_TILES / 2,
        first_shape.cell.1 * CELL_TILES + CELL_TILES / 2,
    );
    assert!((first_shape.center.x - cell_center.x).abs() <= 12);
    assert!((first_shape.center.y - cell_center.y).abs() <= 12);
    assert_ne!(first_shape.center, cell_center);
}

#[test]
fn fixed_key_golden_geometry_count_area_and_boundary_membership_are_pinned() {
    let terrain = recipe_generator([17; 32], 9, crate::GENERATION_RECIPE_VERSION);
    let shape = clearing::geometry(&terrain, TileCoord::new(143, 211)).expect("shape");
    assert_eq!(
        shape,
        Geometry {
            cell: (1, 2),
            center: TileCoord::new(132, 248),
            radii: [
                25, 31, 31, 31, 31, 31, 24, 26, 27, 30, 25, 30, 26, 24, 30, 24
            ],
            vertices: [
                TileCoord::new(157, 248),
                TileCoord::new(160, 259),
                TileCoord::new(153, 269),
                TileCoord::new(143, 276),
                TileCoord::new(132, 279),
                TileCoord::new(121, 276),
                TileCoord::new(116, 264),
                TileCoord::new(108, 258),
                TileCoord::new(105, 248),
                TileCoord::new(105, 237),
                TileCoord::new(115, 231),
                TileCoord::new(121, 221),
                TileCoord::new(132, 222),
                TileCoord::new(141, 226),
                TileCoord::new(153, 227),
                TileCoord::new(154, 239),
            ],
        }
    );
    let cell_origin = TileCoord::new(shape.cell.0 * CELL_TILES, shape.cell.1 * CELL_TILES);
    let cleared_tiles = (cell_origin.y..cell_origin.y + CELL_TILES)
        .flat_map(move |y| {
            (cell_origin.x..cell_origin.x + CELL_TILES).map(move |x| TileCoord::new(x, y))
        })
        .filter(|tile| clearing::contains(&terrain, *tile))
        .count();
    let twice_area = shape
        .vertices
        .iter()
        .zip(shape.vertices.iter().cycle().skip(1))
        .take(shape.vertices.len())
        .map(|(left, right)| {
            i64::from(left.x) * i64::from(right.y) - i64::from(right.x) * i64::from(left.y)
        })
        .sum::<i64>()
        .abs();
    assert_eq!((cleared_tiles, twice_area), (2301, 4569));

    let edge_membership = [
        (TileCoord::new(157, 248), true),
        (TileCoord::new(160, 259), true),
        (TileCoord::new(153, 269), true),
        (TileCoord::new(161, 259), false),
        (TileCoord::new(160, 260), false),
        (TileCoord::new(152, 269), true),
        (TileCoord::new(132, 221), false),
    ];
    for (tile, expected) in edge_membership {
        assert_eq!(clearing::contains(&terrain, tile), expected, "{tile:?}");
    }

    let negative_shape =
        clearing::geometry(&terrain, TileCoord::new(-1, -1)).expect("negative-cell shape");
    assert_eq!(negative_shape.cell, (-1, -1));
    assert_eq!(negative_shape.center, TileCoord::new(-36, -41));
    assert!(clearing::contains(&terrain, negative_shape.center));
    assert!(!clearing::contains(&terrain, TileCoord::new(-1, -1)));
}

#[test]
fn fixed_key_cell_zero_frequency_is_pinned_separately() {
    let frequencies = SAMPLED_KEYS.map(|key| {
        let terrain = recipe_generator(key, 1, crate::GENERATION_RECIPE_VERSION);
        (0..CELL_TILES)
            .flat_map(|y| (0..CELL_TILES).map(move |x| TileCoord::new(x, y)))
            .filter(|tile| clearing::contains(&terrain, *tile))
            .count()
    });
    assert_eq!(
        frequencies,
        [
            2452, 2152, 2297, 2360, 2173, 2349, 2310, 2233, 2122, 2200, 2394, 2207, 2340, 2273,
            2354, 2338,
        ]
    );
}

#[test]
fn every_recipe_five_cell_is_bounded_irregular_and_has_a_clear_17_tile_core() {
    for key in [[3; 32], [29; 32]] {
        let terrain = recipe_generator(key, 1, crate::GENERATION_RECIPE_VERSION);
        for cell_y in 0..3 {
            let mut previous_max_x = None;
            for cell_x in 0..3 {
                let origin = TileCoord::new(cell_x * CELL_TILES, cell_y * CELL_TILES);
                let shape = clearing::geometry(&terrain, origin).expect("cell clearing");
                assert_eq!(shape.cell, (cell_x, cell_y));
                assert!(shape.radii.iter().all(|radius| (24..=31).contains(radius)));
                assert!(shape.radii.iter().any(|radius| *radius != shape.radii[0]));
                let bounds = shape.vertices.iter().fold(
                    (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
                    |bounds, vertex| {
                        (
                            bounds.0.min(vertex.x),
                            bounds.1.min(vertex.y),
                            bounds.2.max(vertex.x),
                            bounds.3.max(vertex.y),
                        )
                    },
                );
                assert!(bounds.0 >= origin.x && bounds.1 >= origin.y);
                assert!(bounds.2 < origin.x + CELL_TILES && bounds.3 < origin.y + CELL_TILES);
                if cell_x > 0
                    && let Some(previous) = previous_max_x
                {
                    assert!(bounds.0 > previous);
                }
                previous_max_x = Some(bounds.2);
                let core = (0..CORE_TILES).flat_map(|dy| {
                    (0..CORE_TILES).map(move |dx| {
                        TileCoord::new(
                            shape.center.x + dx - CORE_TILES / 2,
                            shape.center.y + dy - CORE_TILES / 2,
                        )
                    })
                });
                assert!(
                    core.into_iter()
                        .all(|tile| clearing::contains(&terrain, tile))
                );
            }
        }
    }
}

#[test]
fn recipe_five_suppresses_every_object_candidate_including_the_object_free_core() {
    let terrain = recipe_generator([3; 32], 1, crate::GENERATION_RECIPE_VERSION);
    let legacy = recipe_generator([3; 32], 1, crate::LEGACY_GENERATION_RECIPE_VERSION);
    let shape = clearing::geometry(&terrain, TileCoord::new(40, 40)).expect("cell clearing");
    let core = (0..CORE_TILES).flat_map(|dy| {
        (0..CORE_TILES).map(move |dx| {
            TileCoord::new(
                shape.center.x + dx - CORE_TILES / 2,
                shape.center.y + dy - 8,
            )
        })
    });
    assert!(core.into_iter().all(|tile| {
        let sample = terrain.tile_at(tile).expect("core tile");
        terrain.object_at(tile).is_none()
            && super::resources::candidate(&terrain, tile, sample).is_none()
    }));

    let mut legacy_objects = 0;
    for tile in (0..CELL_TILES).flat_map(|y| (0..CELL_TILES).map(move |x| TileCoord::new(x, y))) {
        if !clearing::contains(&terrain, tile) {
            continue;
        }
        let sample = terrain.tile_at(tile).expect("clearing tile");
        assert!(terrain.object_at(tile).is_none());
        assert!(super::resources::candidate(&terrain, tile, sample).is_none());
        legacy_objects += usize::from(legacy.object_at(tile).is_some());
    }
    assert_eq!(legacy_objects, 625);
}

#[test]
fn recipes_three_and_four_keep_their_pre_clearing_object_layouts() {
    let recipe_three = recipe_generator([71; 32], 5, crate::LEGACY_GENERATION_RECIPE_VERSION);
    let recipe_four = recipe_generator([71; 32], 5, crate::PRIOR_GENERATION_RECIPE_VERSION);
    for recipe in [
        crate::LEGACY_GENERATION_RECIPE_VERSION,
        crate::PRIOR_GENERATION_RECIPE_VERSION,
    ] {
        let terrain = recipe_generator([71; 32], 5, recipe);
        let tile = TileCoord::new(40, 40);
        assert!(clearing::geometry(&terrain, tile).is_none());
        assert!(!clearing::contains(&terrain, tile));
    }
    for (x, y) in [(0, 0), (1, 0), (0, 1)] {
        assert_eq!(
            recipe_three.chunk(x, y).expect("recipe-three chunk"),
            recipe_four.chunk(x, y).expect("recipe-four chunk")
        );
    }
}

#[test]
fn dense_and_provider_clearings_match_with_partial_historical_land_use_exactly() {
    let mut exact = [(0, 0, 0); 3];
    for (index, recipe) in PARITY_RECIPES.into_iter().enumerate() {
        let key = [23; 32];
        let (mut environment, elevation, water, biome, mut pages) = constant_pages();
        let land_use = HistoricalLandUsePage {
            level: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            crop_percent: vec![42],
            grazing_percent: vec![17],
            population_pressure_per_square_kilometer: vec![123],
            coverage: Vec::new(),
        };
        environment.historical_land_use = Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: crate::ordered_land_use_page_root(std::slice::from_ref(
                    &land_use,
                ))
                .expect("land-use root"),
            }],
        });
        pages.0.insert(
            EnvironmentPage::HistoricalLandUse(land_use.clone()).key(),
            Arc::new(EnvironmentPage::HistoricalLandUse(land_use.clone())),
        );
        let dense = prepared_generator(key, 11, recipe, &environment, elevation, water, biome)
            .with_historical_land_use(&environment, vec![land_use])
            .expect("dense land use");
        let provider = provider_generator(key, 11, recipe, environment, pages);
        let mut compared = 0;
        let mut objects = 0;
        let mut wood = 0;
        for cell_y in 0..2 {
            for cell_x in 0..2 {
                let origin = TileCoord::new(cell_x * CELL_TILES, cell_y * CELL_TILES);
                let bounds = if recipe == crate::GENERATION_RECIPE_VERSION {
                    let shape = clearing::geometry(&dense, origin).expect("recipe-five shape");
                    (
                        shape
                            .vertices
                            .iter()
                            .map(|tile| tile.x)
                            .min()
                            .expect("minimum")
                            - 2,
                        shape
                            .vertices
                            .iter()
                            .map(|tile| tile.x)
                            .max()
                            .expect("maximum")
                            + 2,
                        shape
                            .vertices
                            .iter()
                            .map(|tile| tile.y)
                            .min()
                            .expect("minimum")
                            - 2,
                        shape
                            .vertices
                            .iter()
                            .map(|tile| tile.y)
                            .max()
                            .expect("maximum")
                            + 2,
                    )
                } else {
                    (origin.x + 15, origin.x + 80, origin.y + 15, origin.y + 80)
                };
                for y in bounds.2..=bounds.3 {
                    for x in bounds.0..=bounds.1 {
                        let tile = TileCoord::new(x, y);
                        let dense_tile = dense.tile_at(tile).expect("dense tile");
                        let provider_tile = provider
                            .tile_at_with_cancel(tile, &|| false)
                            .expect("provider tile")
                            .expect("provider sample");
                        assert_eq!(dense_tile, provider_tile);
                        let dense_object = dense.object_at(tile);
                        let provider_object = provider
                            .object_at_with_cancel(tile, &|| false)
                            .expect("provider object query");
                        assert_eq!(dense_object, provider_object);
                        compared += 1;
                        objects += usize::from(dense_object.is_some());
                        wood += usize::from(
                            dense_object.is_some_and(|node| node.kind == ResourceKind::Wood),
                        );
                        if clearing::contains(&dense, tile) {
                            assert!(clearing::contains(&provider, tile));
                            assert!(dense_object.is_none());
                        }
                    }
                }
            }
        }
        exact[index] = (compared, objects, wood);
    }
    assert_eq!(
        exact,
        [
            (17_424, 4_613, 4_581),
            (17_424, 4_613, 4_581),
            (14_090, 1_386, 1_375),
        ]
    );
}
