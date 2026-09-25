use super::*;
use aoe_core::{Seed, WorldConfig};

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
