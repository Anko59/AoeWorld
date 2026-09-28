use super::*;
use aoe_map::{
    MapPackage, MapRequest, REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR,
    REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR,
};
use std::collections::VecDeque;

fn corner_fixture() -> (GameWorld, EntityId, WorldPosition, WorldPosition) {
    let config = WorldConfig::new(64, 64, Seed(7)).expect("config");
    let (mut world, id) = GameWorld::with_cavalry(config).expect("world");
    let start = WorldPosition::from_tile_center(TileCoord::new(10, 10)).expect("start");
    let corner = WorldPosition::from_tile_center(TileCoord::new(11, 11)).expect("corner");
    let destination = WorldPosition::from_tile_center(TileCoord::new(12, 11)).expect("target");
    let index = world.lookup[&id];
    world.units[index].state.position = start;
    world.units[index].state.previous_position = start;
    world.units[index].state.moving = true;
    world.units[index].route = VecDeque::from([TileCoord::new(12, 11)]);
    world.units[index].order = Some(MovementOrder {
        origin: start,
        destination,
        waypoint: corner,
        target_tile: TileCoord::new(12, 11),
        segment_length: crate::game_path::segment_length(
            i64::from(corner.x) - i64::from(start.x),
            i64::from(corner.y) - i64::from(start.y),
        ),
        travelled: 0,
        speed_carry: 0,
    });
    world.active_movers.push(id);
    (world, id, corner, destination)
}

#[test]
fn cavalry_uses_three_meters_per_second_as_a_reduced_ratio() {
    let (world, _) =
        GameWorld::with_cavalry(WorldConfig::new(64, 64, Seed(7)).expect("config")).expect("world");
    assert_eq!(world.config().move_speed_subunits_per_tick, 384);
    assert_eq!(world.config().move_speed_subunits_per_tick_denominator, 5);
}

#[test]
fn reference_walking_speed_uses_the_same_rational_model() {
    let config = crate::movement_speed::physical_speed_config(
        WorldConfig::new(64, 64, Seed(7)).expect("config"),
        REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR,
        REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR,
    );
    assert_eq!(config.move_speed_subunits_per_tick, 448);
    assert_eq!(config.move_speed_subunits_per_tick_denominator, 15);
}

#[test]
fn fallback_map_world_uses_the_same_cavalry_ratio() {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    let world = GameWorld::from_map(package).expect("map world");
    assert_eq!(world.config().move_speed_subunits_per_tick, 384);
    assert_eq!(world.config().move_speed_subunits_per_tick_denominator, 5);
}

#[test]
fn corner_consumes_remainder_into_next_waypoint_in_tick_nineteen() {
    let (mut world, id, corner, destination) = corner_fixture();
    for _ in 0..18 {
        world.advance();
    }
    assert_eq!(world.movement_order(id).expect("order").travelled, 1_382);

    world.advance();
    let unit = world.unit(id).expect("unit");
    assert_eq!(unit.position, WorldPosition::new(corner.x + 10, corner.y));
    let order = world.movement_order(id).expect("next order");
    assert_eq!(order.waypoint, destination);
    assert_eq!(order.travelled, 10);
    assert!(order.speed_carry < world.config().move_speed_subunits_per_tick_denominator);

    world.advance();
    assert_eq!(world.movement_order(id).expect("order").travelled, 87);
}

#[test]
fn queued_detour_waypoint_is_consumed_before_planning() {
    let (mut world, id, corner, _) = corner_fixture();
    let index = world.lookup[&id];
    world.units[index].route = VecDeque::from([TileCoord::new(12, 10), TileCoord::new(12, 11)]);
    for _ in 0..19 {
        world.advance();
    }
    let detour = WorldPosition::from_tile_center(TileCoord::new(12, 10)).expect("detour");
    assert_eq!(
        world.unit(id).expect("unit").position,
        WorldPosition::new(corner.x + 7, corner.y - 7)
    );
    assert_eq!(world.movement_order(id).expect("order").waypoint, detour);
}

#[test]
fn short_segment_clamps_at_destination_without_banking_whole_credits() {
    let (mut world, id) =
        GameWorld::with_cavalry(WorldConfig::new(64, 64, Seed(7)).expect("config")).expect("world");
    let start = WorldPosition::from_tile_center(TileCoord::new(10, 10)).expect("start");
    let destination = WorldPosition::new(start.x + 10, start.y);
    let index = world.lookup[&id];
    world.units[index].state.position = start;
    world.units[index].state.previous_position = start;
    world.units[index].state.moving = true;
    world.units[index].order = Some(MovementOrder {
        origin: start,
        destination,
        waypoint: destination,
        target_tile: destination.tile_floor(),
        segment_length: 10,
        travelled: 0,
        speed_carry: 0,
    });
    world.active_movers.push(id);

    world.advance();

    assert_eq!(world.unit(id).expect("unit").position, destination);
    assert!(!world.unit(id).expect("unit").moving);
    assert!(world.movement_order(id).is_none());
}

#[test]
fn off_center_final_tile_reaches_the_exact_destination() {
    let config = WorldConfig::new(64, 64, Seed(7)).expect("config");
    let (mut world, id) = GameWorld::with_cavalry(config).expect("world");
    let start = WorldPosition::from_tile_center(TileCoord::new(10, 10)).expect("start");
    let waypoint = WorldPosition::from_tile_center(TileCoord::new(11, 11)).expect("waypoint");
    let target_center = WorldPosition::from_tile_center(TileCoord::new(12, 11)).expect("target");
    let destination = WorldPosition::new(target_center.x + 100, target_center.y);
    let index = world.lookup[&id];
    world.units[index].state.position = start;
    world.units[index].state.previous_position = start;
    world.units[index].state.moving = true;
    world.units[index].order = Some(MovementOrder {
        origin: start,
        destination,
        waypoint,
        target_tile: destination.tile_floor(),
        segment_length: crate::game_path::segment_length(
            i64::from(waypoint.x) - i64::from(start.x),
            i64::from(waypoint.y) - i64::from(start.y),
        ),
        travelled: 0,
        speed_carry: 0,
    });
    world.active_movers.push(id);

    for _ in 0..100 {
        world.advance();
        if !world.unit(id).expect("unit").moving {
            break;
        }
    }

    assert_eq!(world.unit(id).expect("unit").position, destination);
    assert!(world.movement_order(id).is_none());
}

#[test]
fn redirect_preserves_only_the_fractional_speed_carry() {
    let config = WorldConfig::new(64, 64, Seed(7)).expect("config");
    let (mut world, id) = GameWorld::with_cavalry(config).expect("world");
    let start = world.unit(id).expect("unit").position;
    world
        .issue_move(id, WorldPosition::new(start.x + 5_000, start.y))
        .expect("first move");
    world.advance();
    let carry = world.movement_order(id).expect("first order").speed_carry;
    assert_eq!(carry, 4);

    world
        .issue_move(id, WorldPosition::new(start.x, start.y + 5_000))
        .expect("redirect");
    assert_eq!(
        world
            .movement_order(id)
            .expect("redirect order")
            .speed_carry,
        carry
    );
}

#[test]
fn physical_corner_replay_has_the_same_canonical_hash() {
    let (mut first, id_first, _, _) = corner_fixture();
    let (mut second, id_second, _, _) = corner_fixture();
    for _ in 0..20 {
        first.advance();
        second.advance();
    }
    assert_eq!(first.canonical_hash(), second.canonical_hash());
    assert_eq!(
        first.unit(id_first),
        second.unit(id_second).map(|unit| GameUnit {
            id: id_first,
            ..unit
        })
    );
}

#[test]
fn multi_waypoint_hundred_kilometers_finishes_in_666667_fractional_ticks() {
    let config = WorldConfig::new(64, 64, Seed(11)).expect("config");
    let (mut world, unit) = GameWorld::with_cavalry(config).expect("world");
    let start = world.unit(unit).expect("unit").position;
    let east = WorldPosition::from_tile_center(TileCoord::new(34, 32)).expect("east");
    let west = WorldPosition::from_tile_center(TileCoord::new(32, 32)).expect("west");
    let waypoints = (0..25_000)
        .map(|leg| if leg % 2 == 0 { east } else { west })
        .collect::<Vec<_>>();
    assert!(
        world
            .issue_move_waypoints(unit, &waypoints)
            .expect("predetermined route")
    );

    let distance_subunits = 25_000_u128 * 2_048;
    let expected_ticks = (distance_subunits
        * u128::from(world.config().move_speed_subunits_per_tick_denominator))
    .div_ceil(u128::try_from(world.config().move_speed_subunits_per_tick).expect("positive speed"));
    assert_eq!(expected_ticks, 666_667);
    let denominator = world.config().move_speed_subunits_per_tick_denominator;
    let mut previous_origin = start;
    let mut fractional_boundaries = 0_u64;
    let mut completed_at = None;
    for tick in 1..=expected_ticks {
        world.advance();
        if let Some(order) = world.movement_order(unit) {
            if order.origin != previous_origin {
                assert!(order.speed_carry < denominator);
                if order.speed_carry > 0 {
                    fractional_boundaries += 1;
                }
            }
            previous_origin = order.origin;
        } else {
            completed_at = Some(tick);
            assert_eq!(tick, expected_ticks);
        }
    }

    assert_eq!(completed_at, Some(expected_ticks));
    assert!(fractional_boundaries > 0);
    assert_eq!(world.movement_order(unit), None);
    assert_eq!(world.unit(unit).expect("unit").position, start);
}
