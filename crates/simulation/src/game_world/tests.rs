use super::*;
use aoe_core::{Seed, TILE_GROUND_RADIUS_SUBUNITS};

fn world() -> GameWorld {
    GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap()
}

#[test]
fn fixed_speed_has_equal_distance_for_cardinal_and_diagonal_orders() {
    let mut cardinal = world();
    let mut diagonal = world();
    let start = WorldPosition::new(10 * FIXED_SUBUNITS_PER_TILE, 10 * FIXED_SUBUNITS_PER_TILE);
    let a = cardinal.spawn_unit(PlayerId(0), start).unwrap();
    let b = diagonal.spawn_unit(PlayerId(0), start).unwrap();
    cardinal
        .issue_move(a, WorldPosition::new(start.x + 5_000, start.y))
        .unwrap();
    diagonal
        .issue_move(b, WorldPosition::new(start.x + 5_000, start.y + 5_000))
        .unwrap();
    assert_eq!(cardinal.movement_order(a).unwrap().travelled, 0);
    for _ in 0..8 {
        cardinal.advance();
        diagonal.advance();
    }
    assert_eq!(cardinal.movement_order(a).unwrap().travelled, 1_024);
    assert_eq!(diagonal.movement_order(b).unwrap().travelled, 1_024);
}

#[test]
fn exact_arrival_and_mid_move_redirection_are_deterministic() {
    let mut first = world();
    let mut second = world();
    let start = WorldPosition::new(20_000, 20_000);
    let a = first.spawn_unit(PlayerId(0), start).unwrap();
    let b = second.spawn_unit(PlayerId(0), start).unwrap();
    let target = WorldPosition::new(23_000, 24_000);
    first.issue_move(a, target).unwrap();
    second.issue_move(b, target).unwrap();
    for _ in 0..100 {
        first.advance();
        second.advance();
    }
    assert_eq!(
        first.unit(a),
        second.unit(b).map(|unit| GameUnit { id: a, ..unit })
    );
    assert_eq!(
        first.unit(a).unwrap().position,
        WorldConfig::default().snap_ground_position(target)
    );
    assert!(!first.unit(a).unwrap().moving);
    first
        .issue_move(a, WorldPosition::new(25_000, 24_000))
        .unwrap();
    first.advance();
    assert!(first.unit(a).unwrap().moving);
}

#[test]
fn move_destinations_snap_to_tile_centers() {
    let mut world = world();
    let unit = world
        .spawn_unit(PlayerId(0), WorldPosition::new(20_000, 20_000))
        .unwrap();
    world
        .issue_move(unit, WorldPosition::new(23_000, 24_000))
        .unwrap();
    assert_eq!(
        world.movement_order(unit).unwrap().destination,
        WorldConfig::default().snap_ground_position(WorldPosition::new(23_000, 24_000))
    );
    assert_ne!(
        world.movement_order(unit).unwrap().waypoint,
        world.movement_order(unit).unwrap().destination
    );
}

#[test]
fn default_cavalry_starts_at_a_tile_center() {
    let (world, unit) = GameWorld::default_with_cavalry(Seed(7));
    let position = world.unit(unit).unwrap().position;
    assert_eq!(position.x.rem_euclid(FIXED_SUBUNITS_PER_TILE), 512);
    assert_eq!(position.y.rem_euclid(FIXED_SUBUNITS_PER_TILE), 512);
}

#[test]
fn swap_removal_updates_displaced_bucket_slot() {
    let mut world = world();
    let first = world
        .spawn_unit(PlayerId(0), WorldPosition::new(32_000, 32_000))
        .unwrap();
    let second = world
        .spawn_unit(PlayerId(0), WorldPosition::new(32_100, 32_000))
        .unwrap();
    world
        .issue_move(first, WorldPosition::new(65_000, 32_000))
        .unwrap();
    world.advance();
    world
        .issue_move(second, WorldPosition::new(32_200, 32_000))
        .unwrap();
    let (units, _) = world.query(TileRect::from_xywh(31, 31, 40, 40));
    assert!(units.iter().any(|unit| unit.id == second));
}

#[test]
fn queries_are_stable_and_idle_units_do_not_enter_movement_work() {
    let mut world = world();
    let first = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::new(TILE_GROUND_RADIUS_SUBUNITS, TILE_GROUND_RADIUS_SUBUNITS),
        )
        .unwrap();
    let second = world
        .spawn_unit(PlayerId(0), WorldPosition::new(10_000, 10_000))
        .unwrap();
    let (before, stats) = world.query(TileRect::from_xywh(0, 0, 8, 8));
    assert_eq!(before[0].id, first);
    assert_eq!(stats.returned_units, 1);
    assert_eq!(world.active_mover_count(), 0);
    world
        .issue_move(second, WorldPosition::new(11_000, 10_000))
        .unwrap();
    assert_eq!(world.active_mover_count(), 1);
    let hash = world.canonical_hash();
    world.advance();
    assert_ne!(world.canonical_hash(), hash);
}

#[test]
fn dimensions_do_not_allocate_empty_map_storage() {
    let small = GameWorld::new(WorldConfig::new(1_024, 1_024, Seed(1)).unwrap()).unwrap();
    let large = GameWorld::new(WorldConfig::new(16_384, 16_384, Seed(1)).unwrap()).unwrap();
    assert_eq!(small.unit_count(), large.unit_count());
    assert_eq!(small.occupied_chunk_count(), large.occupied_chunk_count());
}

#[test]
fn deterministic_population_supports_hotspots_and_sparse_extents() {
    let config = WorldConfig::new(16_384, 16_384, Seed(19)).unwrap();
    let world = GameWorld::with_population_in_extent(config, 8_000, 1_000, 4, 1_024).unwrap();
    let (units, stats) = world.query(TileRect::from_xywh(0, 0, 128, 128));
    assert_eq!(world.unit_count(), 8_000);
    assert!(units.len() >= 1_000);
    assert!(stats.candidate_units >= stats.returned_units);
}
