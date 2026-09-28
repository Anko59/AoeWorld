use super::*;
use aoe_core::{Seed, WorldConfig};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid, MapRequest,
    PreparedEnvironment, ProjectionMetadata, PyramidLevel, ordered_page_root,
};
use std::fs;

fn elevation(level: u8) -> ElevationPage {
    let side = if level == 0 { 2 } else { 1 };
    ElevationPage {
        level,
        x: 0,
        y: 0,
        width: side,
        height: side,
        geographic_height_centimeters: vec![0; usize::from(side).pow(2)],
    }
}

fn package() -> MapPackage {
    MapPackage::with_prepared_environment(
        1,
        MapRequest {
            requested_side_meters: 3_840,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 1_920_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: (0..2)
                    .map(|level| PyramidLevel {
                        samples_per_axis: if level == 0 { 2 } else { 1 },
                        ordered_page_root: ordered_page_root(&[elevation(level)]).unwrap(),
                    })
                    .collect(),
            },
            water: None,
            vegetation: None,
            historical_land_use: None,
            hydrology_evidence: None,
        },
    )
    .unwrap()
}

fn write_elevation_pages(directory: &std::path::Path, package: &MapPackage) {
    let root = directory
        .join("pages")
        .join(package.content_hash_hex())
        .join("elevation");
    fs::create_dir_all(&root).unwrap();
    for level in 0..2 {
        let page = elevation(level);
        fs::write(
            root.join(format!("{level}-0-0.json")),
            serde_json::to_vec(&page).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn continuous_source_free_run_completes_all_repeated_waypoints() {
    let directory = tempfile::tempdir().expect("package directory");
    let package = package();
    write_elevation_pages(directory.path(), &package);
    let generator = package.generator();
    let world = GameWorld::from_map(package.clone()).expect("movement world");
    let config = world.config();
    let route = [
        TileCoord::new(10, 10),
        TileCoord::new(32, 32),
        TileCoord::new(64, 64),
        TileCoord::new(128, 128),
    ]
    .into_iter()
    .find_map(|start| {
        super::super::route::plan_fixed_repeated_route(world.terrain(), config, start).ok()
    })
    .expect("open fixed route");
    let movement_provider =
        PageResidency::open(directory.path(), &package, &|| false).expect("movement provider");
    let replay_provider =
        PageResidency::open(directory.path(), &package, &|| false).expect("replay provider");
    let mut progress_ticks = Vec::new();
    let mut progress = |progress: SourceQualificationProgress| progress_ticks.push(progress.tick);
    let mut rss = ProcessRssSampler::start();

    let run = exercise_movement(MovementInput {
        package: &package,
        generator: &generator,
        movement_provider,
        replay_provider,
        initial_peak_resident_pages: 128,
        max_ticks: 666_667,
        route,
        progress: &mut progress,
        rss: &mut rss,
    })
    .expect("continuous movement run");

    assert_eq!(run.movement_ticks, 666_667);
    assert_eq!(run.route_moved_meters, 100_000.0);
    assert_eq!(run.replay_moved_meters, 100_000.0);
    assert_eq!(run.route_movement_leg_count, 25_000);
    assert_eq!(run.replay_movement_leg_count, 25_000);
    assert_eq!(run.route_checkpoint_count, 81);
    assert_eq!(run.movement_replay_comparison_count, 83);
    assert_eq!(progress_ticks.len(), 13);
}

#[test]
fn exact_distance_requires_identical_replay_distance() {
    let config = WorldConfig::new(64, 64, Seed(9)).expect("config");
    let route = super::super::route::plan_fixed_repeated_route(
        &aoe_simulation::Terrain::uniform(config.seed.0),
        config,
        TileCoord::new(10, 10),
    )
    .expect("fixed route");
    validate_distance(route, 100_000.0, 100_000.0).expect("exact threshold passes");

    let short =
        validate_distance(route, 99_999.999, 99_999.999).expect_err("short route must fail closed");
    assert!(matches!(
        short,
        SourceQualificationError::MovementDistanceMismatch {
            expected: 100_000.0,
            observed: 99_999.999
        }
    ));
    let long = validate_distance(route, 100_000.001, 100_000.001)
        .expect_err("long route must fail closed");
    assert!(matches!(
        long,
        SourceQualificationError::MovementDistanceMismatch {
            expected: 100_000.0,
            observed: 100_000.001
        }
    ));

    let diverged = validate_distance(route, 100_000.0, 99_999.0)
        .expect_err("replay distance mismatch must fail closed");
    assert!(matches!(
        diverged,
        SourceQualificationError::ReplayDistanceMismatch {
            route_meters: 100_000.0,
            replay_meters: 99_999.0
        }
    ));
}

#[test]
fn source_runner_accepts_only_the_exact_continuous_tick_count() {
    let route = super::super::route::plan_fixed_repeated_route(
        &aoe_simulation::Terrain::uniform(9),
        WorldConfig::new(64, 64, Seed(9)).expect("config"),
        TileCoord::new(10, 10),
    )
    .expect("fixed route");
    let config = GameWorld::with_cavalry(WorldConfig::new(64, 64, Seed(9)).expect("config"))
        .expect("cavalry world")
        .0
        .config();
    let expected_ticks = route.expected_ticks(config).expect("continuous arithmetic");
    assert_eq!(expected_ticks, 666_667);
    let exact = exact_run(route, expected_ticks);
    exact.validate(config).expect("exact evidence passes");

    for observed in [expected_ticks - 1, expected_ticks + 1] {
        let mismatch = exact_run(route, observed)
            .validate(config)
            .expect_err("non-exact ticks must fail closed");
        assert!(matches!(
            mismatch,
            SourceQualificationError::MovementTickMismatch {
                expected: 666_667,
                observed: _
            }
        ));
    }
}

fn exact_run(route: super::super::route::FixedRoute, movement_ticks: u64) -> MovementRun {
    MovementRun {
        start_tile: route.start,
        route,
        movement_ticks,
        route_moved_meters: 100_000.0,
        replay_moved_meters: 100_000.0,
        route_checkpoint_count: 81,
        movement_replay_comparison_count: 83,
        route_movement_leg_count: 25_000,
        replay_movement_leg_count: 25_000,
        peak_resident_pages: 128,
        navigation_cache_peaks: NavigationCachePeaks::default(),
        route_hash: [7; 32],
        replay_hash: [7; 32],
    }
}

#[test]
fn deterministic_replay_has_identical_hash_and_position() {
    ensure_replay([7; 32], [7; 32], 8_192).expect("identical replay");
    let error =
        ensure_replay([7; 32], [8; 32], 16_384).expect_err("divergent replay must fail closed");
    assert!(matches!(
        error,
        SourceQualificationError::ReplayDiverged(16_384)
    ));

    let config = WorldConfig::new(64, 64, Seed(9)).expect("config");
    let (mut route_world, route_unit) = GameWorld::with_cavalry(config).expect("route world");
    let (mut replay_world, replay_unit) = GameWorld::with_cavalry(config).expect("replay world");
    let start = WorldPosition::from_tile_center(TileCoord::new(32, 32)).expect("start");
    let destination = WorldPosition::from_tile_center(TileCoord::new(34, 32)).expect("target");
    route_world
        .issue_move(route_unit, destination)
        .expect("route move");
    replay_world
        .issue_move(replay_unit, destination)
        .expect("replay move");
    for _ in 0..20 {
        route_world.advance();
        replay_world.advance();
    }
    ensure_replay(
        route_world.canonical_hash(),
        replay_world.canonical_hash(),
        20,
    )
    .expect("identically advanced worlds");
    assert_eq!(
        route_world.unit(route_unit).expect("route unit").position,
        replay_world
            .unit(replay_unit)
            .expect("replay unit")
            .position
    );
    assert_ne!(start, destination);
}

#[test]
fn movement_validation_and_distance_transitions_fail_closed() {
    let route = super::super::route::plan_fixed_repeated_route(
        &aoe_simulation::Terrain::uniform(9),
        WorldConfig::new(64, 64, Seed(9)).expect("config"),
        TileCoord::new(10, 10),
    )
    .expect("fixed route");
    let mut short_route = route;
    short_route.repetitions -= 1;
    assert!(matches!(
        validate_distance(short_route, 100_000.0, 100_000.0),
        Err(SourceQualificationError::MovementDistanceMismatch { .. })
    ));

    let mut wrong_legs = exact_run(route, 666_667);
    wrong_legs.route_movement_leg_count -= 1;
    assert!(matches!(
        wrong_legs.validate(
            GameWorld::with_cavalry(WorldConfig::new(64, 64, Seed(9)).expect("config"))
                .expect("config")
                .0
                .config()
        ),
        Err(SourceQualificationError::MovementRepetitionMismatch { .. })
    ));

    let before = WorldPosition::from_tile_center(TileCoord::new(10, 10)).expect("before");
    let waypoint = WorldPosition::from_tile_center(TileCoord::new(11, 10)).expect("waypoint");
    let after = WorldPosition::from_tile_center(TileCoord::new(12, 10)).expect("after");
    let order = aoe_simulation::MovementOrder {
        origin: before,
        destination: after,
        waypoint,
        target_tile: TileCoord::new(12, 10),
        segment_length: 2,
        travelled: 1,
        speed_carry: 0,
    };
    assert_eq!(advanced_distance(before, None, after, None), 2_048.0);
    assert_eq!(advanced_distance(before, Some(order), after, None), 2_048.0);
    assert_eq!(
        advanced_distance(before, Some(order), after, Some(order)),
        2_048.0
    );
    let changed = aoe_simulation::MovementOrder {
        waypoint: WorldPosition::from_tile_center(TileCoord::new(11, 11)).expect("changed"),
        ..order
    };
    assert!(advanced_distance(before, Some(changed), after, Some(order)) > 2_048.0);
}
