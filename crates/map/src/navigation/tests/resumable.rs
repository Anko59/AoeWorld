use super::*;
use crate::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentPageError, FieldPyramid,
    PotentialBiomePage, PreparedEnvironment, PyramidLevel, Ratio, RoutePlanner, RoutePlannerPoll,
    WaterPage, ordered_biome_page_root, ordered_page_root, ordered_water_page_root,
};

fn flat_terrain() -> MapChunkGenerator {
    flat_terrain_with_width(128)
}

fn flat_terrain_with_width(width: i32) -> MapChunkGenerator {
    let level_zero = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0; 4],
    };
    let overview = ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&level_zero))
                        .expect("level-zero root"),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&overview))
                        .expect("overview root"),
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
    };
    MapChunkGenerator::new([0; 32], 0, width)
        .with_prepared_elevation(
            Ratio::new(1, 1).expect("compression"),
            &environment,
            vec![level_zero, overview],
        )
        .expect("flat terrain")
}

fn cleared_overlay(terrain: &MapChunkGenerator) -> ResourceOverlay {
    cleared_overlay_with_width(terrain, 128)
}

fn cleared_overlay_with_width(terrain: &MapChunkGenerator, width: i32) -> ResourceOverlay {
    let mut overlay = ResourceOverlay::default();
    for y in 0..width {
        for x in 0..width {
            if let Some(node) = terrain.object_at(TileCoord::new(x, y)) {
                overlay
                    .deplete(terrain, node.id, node.initial_amount)
                    .expect("resource");
            }
        }
    }
    overlay
}

fn enclosed_local_terrain() -> MapChunkGenerator {
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0; 4],
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        ocean_coverage_percent: vec![0, 100, 100, 0],
        inland_coverage_percent: vec![0; 4],
    };
    let biome = PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        potential_biome_class: vec![27; 4],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation))
                    .expect("elevation root"),
            }],
        },
        water: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: ordered_water_page_root(std::slice::from_ref(&water))
                    .expect("water root"),
            }],
        }),
        vegetation: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: ordered_biome_page_root(std::slice::from_ref(&biome))
                    .expect("biome root"),
            }],
        }),
        historical_land_use: None,
    };
    MapChunkGenerator::new([0; 32], 0, 2)
        .with_prepared_elevation(
            Ratio::new(1, 1).expect("compression"),
            &environment,
            vec![elevation],
        )
        .expect("elevation")
        .with_prepared_water(&environment, vec![water])
        .expect("water")
        .with_prepared_biomes(&environment, vec![biome])
        .expect("biome")
}

#[test]
fn fixed_route_planner_resumes_the_same_frontier() {
    let terrain = flat_terrain();
    let overlay = cleared_overlay(&terrain);
    let mut planner = RoutePlanner::new(TileCoord::new(1, 1), TileCoord::new(20, 1), 4_096);
    let mut previous_expansions = 0;
    let mut polls = 0;
    let path = 'polls: loop {
        polls += 1;
        assert!(polls <= 8_192);
        assert!(previous_expansions <= 4_096);
        let result = planner.poll(&terrain, &overlay, 1, &|| false);
        assert!(planner.expansions() >= previous_expansions);
        previous_expansions = planner.expansions();
        match result {
            RoutePlannerPoll::Pending => continue,
            RoutePlannerPoll::Path(path) => break 'polls path,
            other => panic!("fixed flat route failed: {other:?}"),
        }
    };
    assert_eq!(path.tiles.first(), Some(&TileCoord::new(1, 1)));
    assert_eq!(path.tiles.last(), Some(&TileCoord::new(20, 1)));
    assert!(planner.is_terminal());
}

#[test]
fn equivalent_budget_partitions_reach_the_same_route_and_planner_hash() {
    let terrain = flat_terrain();
    let overlay = cleared_overlay(&terrain);
    let origin = TileCoord::new(1, 1);
    let destination = TileCoord::new(20, 1);
    let mut partitioned = RoutePlanner::new(origin, destination, 4_096);
    let mut partitioned_result = RoutePlannerPoll::Pending;
    for _ in 0..8_192 {
        let mut before = blake3::Hasher::new();
        partitioned.update_hash(&mut before);
        partitioned_result = partitioned.poll(&terrain, &overlay, 1, &|| false);
        let mut after = blake3::Hasher::new();
        partitioned.update_hash(&mut after);
        if matches!(partitioned_result, RoutePlannerPoll::Pending) {
            assert_ne!(before.finalize(), after.finalize());
        } else {
            break;
        }
    }
    assert!(partitioned.is_terminal());

    let mut single = RoutePlanner::new(origin, destination, 4_096);
    let single_result = single.poll(&terrain, &overlay, 8_192, &|| false);
    assert_eq!(partitioned_result, single_result);
    let mut partitioned_hash = blake3::Hasher::new();
    partitioned.update_hash(&mut partitioned_hash);
    let mut single_hash = blake3::Hasher::new();
    single.update_hash(&mut single_hash);
    assert_eq!(partitioned_hash.finalize(), single_hash.finalize());
}

#[test]
fn planner_reports_search_limit_without_becoming_a_cacheable_unreachable() {
    let terrain = flat_terrain();
    let overlay = cleared_overlay(&terrain);
    let mut planner = RoutePlanner::new(TileCoord::new(1, 1), TileCoord::new(20, 1), 1);
    assert_eq!(
        planner.poll(&terrain, &overlay, 1, &|| false),
        RoutePlannerPoll::Pending
    );
    assert_eq!(
        planner.poll(&terrain, &overlay, 1, &|| false),
        RoutePlannerPoll::SearchLimit
    );
    assert_ne!(
        planner.poll(&terrain, &overlay, 1, &|| false),
        RoutePlannerPoll::Unreachable
    );
}

#[test]
fn portal_attempt_budget_exhaustion_does_not_restart_a_stale_frontier() {
    let terrain = flat_terrain_with_width(512);
    let overlay = cleared_overlay_with_width(&terrain, 512);
    let mut planner = RoutePlanner::new(TileCoord::new(1, 1), TileCoord::new(300, 1), 1);
    assert_eq!(
        planner.poll(&terrain, &overlay, 1, &|| false),
        RoutePlannerPoll::Pending
    );
    let mut before = blake3::Hasher::new();
    planner.update_hash(&mut before);
    assert_eq!(
        planner.poll(&terrain, &overlay, 1, &|| false),
        RoutePlannerPoll::SearchLimit
    );
    let mut after = blake3::Hasher::new();
    planner.update_hash(&mut after);
    assert_ne!(before.finalize(), after.finalize());
    assert_eq!(
        planner.poll(&terrain, &overlay, 1, &|| false),
        RoutePlannerPoll::SearchLimit
    );
}

#[test]
fn cancellation_is_a_typed_environment_failure() {
    let terrain = flat_terrain();
    let overlay = cleared_overlay(&terrain);
    let mut planner = RoutePlanner::new(TileCoord::new(1, 1), TileCoord::new(20, 1), 4_096);
    assert_eq!(
        planner.poll(&terrain, &overlay, 1, &|| true),
        RoutePlannerPoll::Environment(EnvironmentPageError::Cancelled)
    );
}

#[test]
fn zero_budget_poll_honors_cancellation_after_initialization() {
    let terrain = flat_terrain();
    let overlay = cleared_overlay(&terrain);
    let mut planner = RoutePlanner::new(TileCoord::new(1, 1), TileCoord::new(20, 1), 4_096);
    assert_eq!(
        planner.poll(&terrain, &overlay, 0, &|| false),
        RoutePlannerPoll::Pending
    );
    assert_eq!(
        planner.poll(&terrain, &overlay, 0, &|| true),
        RoutePlannerPoll::Environment(EnvironmentPageError::Cancelled)
    );
}

#[test]
fn exhausted_direct_search_becomes_unreachable_once() {
    let terrain = enclosed_local_terrain();
    let overlay = ResourceOverlay::default();
    let mut planner = RoutePlanner::new(TileCoord::new(0, 0), TileCoord::new(1, 1), 4_096);
    assert_eq!(
        planner.poll(&terrain, &overlay, 64, &|| false),
        RoutePlannerPoll::Unreachable
    );
    assert!(planner.is_terminal());
    assert_eq!(
        planner.poll(&terrain, &overlay, 64, &|| false),
        RoutePlannerPoll::Unreachable
    );
}
