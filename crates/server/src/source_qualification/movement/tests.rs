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
    assert!(short.to_string().contains("only 99999.999 m"));

    let diverged = validate_distance(route, 100_000.0, 99_999.0)
        .expect_err("replay distance mismatch must fail closed");
    assert!(diverged.to_string().contains("only 100000.000 m"));
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
