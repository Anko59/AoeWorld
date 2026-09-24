use super::*;
use aoe_core::Seed;
use aoe_map::{
    ElevationPage, FieldPyramid, MapChunkGenerator, MapPackage, MapRequest, PotentialBiomePage,
    PreparedEnvironment, PyramidLevel, Ratio, ResourceOverlay, WaterPage, ordered_biome_page_root,
    ordered_page_root, ordered_water_page_root,
};

#[test]
fn start_search_contract_constants_are_exact() {
    assert_eq!(START_CLEAR_RADIUS, 2);
    assert_eq!((START_CLEAR_RADIUS * 2 + 1).pow(2), 25);
    assert_eq!(START_REACHABLE_TILES, 256);
    assert_eq!(START_SEARCH_CHUNKS, 64);
}

#[test]
fn rings_only_visit_their_perimeter_once() {
    for radius in 1..100 {
        let ring = ring_chunks(TileCoord::new(4, 7), radius);
        assert_eq!(ring.len(), radius as usize * 8);
        assert_eq!(
            ring.iter().copied().collect::<BTreeSet<_>>().len(),
            ring.len()
        );
    }
}

#[test]
fn cancellation_and_budget_are_not_proof_of_absence() {
    let terrain = Terrain::uniform(1);
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
    assert_eq!(
        terrain.search_start(config, START_SEARCH_CHUNKS, || true),
        StartSearchResult::Cancelled
    );
    assert_eq!(
        terrain.search_start(config, 0, || false),
        StartSearchResult::LimitReached
    );
    for max_chunks in [1, START_SEARCH_CHUNKS] {
        assert_eq!(
            terrain.search_start(config, max_chunks, || false),
            StartSearchResult::Found(TileCoord::new(31, 31))
        );
    }
}

#[test]
fn centered_candidate_keys_round_trip_across_odd_and_even_boundaries() {
    for width_tiles in [1_i32, 2, 3, 4, 31, 32, 33, 64, 512] {
        let config = WorldConfig::new(width_tiles, width_tiles, Seed(1)).expect("config");
        for coordinate in 0..width_tiles {
            let mirrored = width_tiles - 1 - coordinate;
            assert_eq!(
                centered_distance_squared(coordinate, width_tiles),
                centered_distance_squared(mirrored, width_tiles),
                "cell-center distance is symmetric for width {width_tiles}"
            );
        }
        let expected_edge = u64::try_from(width_tiles - 1)
            .expect("positive map width")
            .pow(2);
        assert_eq!(centered_distance_squared(0, width_tiles), expected_edge);
        assert_eq!(
            centered_distance_squared(width_tiles - 1, width_tiles),
            expected_edge
        );
        let center = TileCoord::new((width_tiles - 1) / 2, (width_tiles - 1) / 2);
        assert_eq!(
            start_key(center, config).0,
            centered_distance_squared(center.x, width_tiles)
                + centered_distance_squared(center.y, width_tiles)
        );
    }
}

#[test]
fn footprint_boundary_candidates_fail_closed_for_every_supported_recipe() {
    let terrain = Terrain::uniform(1);
    let config = WorldConfig::new(5, 5, Seed(1)).expect("config");
    for recipe in [
        LEGACY_START_RECIPE,
        RECIPE_4_START_RECIPE,
        RECIPE_5_START_RECIPE,
    ] {
        assert_eq!(
            terrain.search_start_for_recipe(config, recipe, START_SEARCH_CHUNKS, || false),
            Ok(StartSearchResult::Unavailable),
            "recipe {recipe} cannot wrap the 5x5 footprint across map boundaries"
        );
    }
}

#[test]
fn open_fixture_retains_recipe_three_and_recipe_four_compatibility() {
    let terrain = Terrain::uniform(1);
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
    let expected = StartSearchResult::Found(TileCoord::new(31, 31));
    for recipe in [LEGACY_START_RECIPE, RECIPE_4_START_RECIPE] {
        let selected = terrain
            .search_start_for_recipe(config, recipe, START_SEARCH_CHUNKS, || false)
            .expect("open-fixture recipe search");
        assert_eq!(selected, expected, "recipe {recipe} selection");
        assert_eq!(
            terrain.search_start_for_recipe(config, recipe, START_SEARCH_CHUNKS, || false),
            Ok(selected),
            "recipe {recipe} selection is deterministic"
        );
        assert_start_contract(&terrain, config, selected);
    }
    assert_eq!(
        terrain.search_start_checked(config, START_SEARCH_CHUNKS, || false),
        Ok(expected),
        "the unversioned API retains legacy recipe-3 semantics"
    );
    assert_eq!(terrain.starting_tile(config), Some(TileCoord::new(31, 31)));
}

#[test]
fn dense_recipe_five_temperate_start_preserves_the_fixed_contract() {
    let terrain = Terrain::Map {
        generator: flat_temperate_generator(RECIPE_5_START_RECIPE),
        overlay: ResourceOverlay::default(),
    };
    let config = WorldConfig::new(512, 512, Seed(1)).expect("config");
    let selected = terrain
        .search_start_for_recipe(config, RECIPE_5_START_RECIPE, START_SEARCH_CHUNKS, || false)
        .expect("recipe 5 search");
    assert_eq!(
        terrain
            .search_start_for_recipe(config, RECIPE_5_START_RECIPE, START_SEARCH_CHUNKS, || false,),
        Ok(selected),
        "recipe 5 selection is deterministic"
    );
    assert_start_contract(&terrain, config, selected);
}

#[test]
fn dense_recipe_three_and_four_reach_the_fixed_search_limit() {
    let config = WorldConfig::new(512, 512, Seed(1)).expect("config");
    for recipe in [LEGACY_START_RECIPE, RECIPE_4_START_RECIPE] {
        let terrain = Terrain::Map {
            generator: flat_temperate_generator(recipe),
            overlay: ResourceOverlay::default(),
        };
        assert_eq!(
            terrain.search_start_for_recipe(config, recipe, START_SEARCH_CHUNKS, || false),
            Ok(StartSearchResult::LimitReached),
            "dense recipe {recipe} remains bounded instead of weakening the contract"
        );
    }
}

#[test]
fn start_search_rejects_unknown_generation_recipes() {
    let terrain = Terrain::uniform(1);
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
    assert_eq!(
        terrain.search_start_for_recipe(config, 99, START_SEARCH_CHUNKS, || false),
        Err(EnvironmentPageError::Invalid)
    );
}

fn assert_start_contract(terrain: &Terrain, config: WorldConfig, selected: StartSearchResult) {
    let StartSearchResult::Found(tile) = selected else {
        unreachable!("start contract fixture returned a start above")
    };
    let mut cache = StartPassabilityCache::new(terrain, config, &|| false);
    assert!(clear_starting_area(&mut cache, tile).expect("5x5 footprint"));
    assert!(
        cache
            .reaches_required_tiles(tile)
            .expect("reachable-area threshold")
    );
    assert!(
        terrain.reachable_tiles(tile, config, START_REACHABLE_TILES + 1) >= START_REACHABLE_TILES,
        "start must reach at least {START_REACHABLE_TILES} tiles"
    );
}

fn flat_temperate_generator(generation_recipe: u16) -> MapChunkGenerator {
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
    let vegetation = PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        potential_biome_class: vec![9],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1_000,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation))
                    .expect("elevation root"),
            }],
        },
        water: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: ordered_water_page_root(std::slice::from_ref(&water))
                    .expect("water root"),
            }],
        }),
        vegetation: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: ordered_biome_page_root(std::slice::from_ref(&vegetation))
                    .expect("vegetation root"),
            }],
        }),
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let request = MapRequest {
        requested_side_meters: 30_720,
        ..MapRequest::default()
    };
    let mut package = MapPackage::new(1, request, Vec::new()).expect("fixture package");
    package.generation_recipe_version = generation_recipe;
    package
        .generator()
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
        .with_prepared_biomes(&environment, vec![vegetation])
        .expect("flat vegetation")
}
