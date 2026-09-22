use super::*;
use aoe_core::Seed;
use aoe_map::{
    ElevationPage, FieldPyramid, MapChunkGenerator, PotentialBiomePage, PreparedEnvironment,
    PyramidLevel, Ratio, ResourceOverlay, WaterPage, ordered_biome_page_root, ordered_page_root,
    ordered_water_page_root,
};

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
        terrain.search_start(config, 64, || true),
        StartSearchResult::Cancelled
    );
    assert_eq!(
        terrain.search_start(config, 0, || false),
        StartSearchResult::LimitReached
    );
    assert_eq!(
        terrain.search_start(config, 1, || false),
        StartSearchResult::Found(TileCoord::new(31, 31))
    );
}

#[test]
fn recipe_four_uses_three_by_three_but_keeps_legacy_search_unchanged() {
    let terrain = Terrain::Map {
        generator: flat_temperate_generator(),
        overlay: ResourceOverlay::default(),
    };
    let config = WorldConfig::new(512, 512, Seed(1)).expect("config");
    let recipe_four = terrain
        .search_start_for_recipe(config, RECIPE_4_START_RECIPE, 64, || false)
        .expect("recipe 4 search");
    assert!(matches!(recipe_four, StartSearchResult::Found(_)));
    assert_eq!(
        terrain.search_start_for_recipe(config, RECIPE_4_START_RECIPE, 64, || false),
        Ok(recipe_four),
        "recipe 4 selection is deterministic"
    );
    let recipe_three = terrain
        .search_start_for_recipe(config, LEGACY_START_RECIPE, 64, || false)
        .expect("recipe 3 search");
    assert!(matches!(recipe_three, StartSearchResult::Found(_)));
    assert_eq!(
        terrain.search_start_checked(config, 64, || false),
        Ok(recipe_three),
        "the unversioned API retains legacy semantics"
    );
    let mut cache = StartPassabilityCache::new(&terrain, config, &|| false);
    let StartSearchResult::Found(recipe_four_tile) = recipe_four else {
        unreachable!("recipe 4 returned a start above")
    };
    assert!(
        clear_starting_area(&mut cache, recipe_four_tile, RECIPE_4_START_CLEAR_RADIUS)
            .expect("recipe 4 footprint")
    );
    assert!(
        !clear_starting_area(&mut cache, recipe_four_tile, LEGACY_START_CLEAR_RADIUS)
            .expect("recipe 4 legacy comparison"),
        "recipe 4 can select a site that does not meet the legacy 5×5 footprint"
    );
    assert!(
        cache
            .reaches_required_tiles(recipe_four_tile)
            .expect("recipe 4 reachable area")
    );
    let StartSearchResult::Found(recipe_three_tile) = recipe_three else {
        unreachable!("recipe 3 returned a start above")
    };
    assert!(
        clear_starting_area(&mut cache, recipe_three_tile, LEGACY_START_CLEAR_RADIUS)
            .expect("recipe 3 footprint")
    );
    assert!(
        cache
            .reaches_required_tiles(recipe_three_tile)
            .expect("recipe 3 reachable area")
    );
}

#[test]
fn start_search_rejects_unknown_generation_recipes() {
    let terrain = Terrain::uniform(1);
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
    assert_eq!(
        terrain.search_start_for_recipe(config, 99, 64, || false),
        Err(EnvironmentPageError::Invalid)
    );
}

fn flat_temperate_generator() -> MapChunkGenerator {
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
        potential_biome_class: vec![16],
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
    };
    MapChunkGenerator::new([3; 32], 1, 512)
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
