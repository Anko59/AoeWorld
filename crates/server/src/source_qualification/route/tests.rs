use super::*;
use aoe_core::{Seed, WorldPosition};
use std::collections::BTreeSet;

#[derive(Debug)]
struct StaticLeg {
    blocked_edges: BTreeSet<((i32, i32), (i32, i32))>,
}

impl StaticLeg {
    fn open() -> Self {
        Self {
            blocked_edges: BTreeSet::new(),
        }
    }

    fn block(&mut self, from: (i32, i32), to: (i32, i32)) {
        self.blocked_edges.insert((from, to));
        self.blocked_edges.insert((to, from));
    }
}

impl FixedLegQuery for StaticLeg {
    fn passable(&self, _tile: TileCoord) -> Result<bool, EnvironmentPageError> {
        Ok(true)
    }

    fn crossable(&self, from: TileCoord, to: TileCoord) -> Result<bool, EnvironmentPageError> {
        Ok(!self
            .blocked_edges
            .contains(&((from.x, from.y), (to.x, to.y))))
    }
}

#[test]
fn preferred_fixed_leg_is_start_to_start_plus_two_east() {
    let query = StaticLeg::open();
    let start = TileCoord::new(10, 10);
    let route = plan_fixed_repeated_route_with(&query, start).expect("open preferred leg");

    assert_eq!(route.start, start);
    assert_eq!(route.alternate, TileCoord::new(12, 10));
    assert_eq!(route.offset, (2, 0));
    assert_eq!(route.leg_length_tiles, 2);
    assert_eq!(route.leg_length_meters, 4.0);
    assert_eq!(route.spatial_extent_tiles(), [2, 0]);
}

#[test]
fn fixed_leg_fail_closed_when_every_cardinal_leg_is_blocked() {
    let mut query = StaticLeg::open();
    for offset in CARDINAL_OFFSETS {
        let mut from = (10, 10);
        let x_step = offset.0.signum();
        let y_step = offset.1.signum();
        for _ in 0..(offset.0.unsigned_abs() + offset.1.unsigned_abs()) {
            let to = (from.0 + x_step, from.1 + y_step);
            query.block(from, to);
            from = to;
        }
    }

    let error = plan_fixed_repeated_route_with(&query, TileCoord::new(10, 10))
        .expect_err("all fixed legs blocked must fail closed");
    let SourceQualificationError::FixedRouteLegUnavailable { start, diagnostic } = error else {
        panic!("unexpected error: {error}");
    };
    assert_eq!(start, [10, 10]);
    assert!(diagnostic.contains("preferred=(2, 0)"));
    assert!(diagnostic.contains("offset=(0, 2),crossable=false"));
    assert!(diagnostic.contains("offset=(-2, 0),crossable=false"));
    assert!(diagnostic.contains("offset=(0, -2),crossable=false"));
}

#[test]
fn deterministic_fallback_selects_south_when_preferred_east_leg_is_blocked() {
    let mut query = StaticLeg::open();
    query.block((10, 10), (11, 10));
    query.block((11, 10), (12, 10));
    let start = TileCoord::new(10, 10);

    let first = plan_fixed_repeated_route_with(&query, start).expect("fallback leg");
    let second = plan_fixed_repeated_route_with(&query, start).expect("fallback leg");

    assert_eq!(first, second);
    assert_eq!(first.offset, (0, 2));
    assert_eq!(first.alternate, TileCoord::new(10, 12));
}

#[test]
fn exact_accumulated_distance_and_repetition_count_reach_100km() {
    let route = plan_fixed_repeated_route_with(&StaticLeg::open(), TileCoord::new(10, 10))
        .expect("preferred leg");

    assert_eq!(route.leg_length_meters, 4.0);
    assert_eq!(route.repetitions, 25_000);
    assert_eq!(
        route.accumulated_distance_meters(route.repetitions),
        100_000.0
    );
    assert!(route.accumulated_distance_meters(route.repetitions - 1) < 100_000.0);
    assert_eq!(route.destination_for_leg(0), route.alternate);
    assert_eq!(route.destination_for_leg(1), route.start);
    assert_eq!(route.destination_for_leg(2), route.alternate);
}

#[test]
fn max_tick_bound_fails_closed_before_movement() {
    let route = plan_fixed_repeated_route_with(&StaticLeg::open(), TileCoord::new(10, 10))
        .expect("preferred leg");
    let mut config = WorldConfig::new(64, 64, Seed(4)).expect("config");
    config.move_speed_subunits_per_tick = 1;
    config.move_speed_subunits_per_tick_denominator = 1;
    let required = route.expected_ticks(config).expect("bounded arithmetic");
    assert_eq!(required, 51_200_000);

    let error = route
        .ensure_tick_bound(config, required - 1)
        .expect_err("tick bound must fail closed");
    assert!(matches!(
        error,
        SourceQualificationError::FixedRouteTickBound {
            required: 51_200_000,
            maximum
        } if maximum == required - 1
    ));
}

#[test]
fn continuous_expected_ticks_preserve_fractional_speed_across_all_legs() {
    let route = plan_fixed_repeated_route_with(&StaticLeg::open(), TileCoord::new(10, 10))
        .expect("preferred leg");
    let config =
        aoe_simulation::GameWorld::with_cavalry(WorldConfig::new(64, 64, Seed(9)).expect("config"))
            .expect("cavalry world")
            .0
            .config();

    assert_eq!(config.tick_hz, 20);
    assert_eq!(config.move_speed_subunits_per_tick, 384);
    assert_eq!(config.move_speed_subunits_per_tick_denominator, 5);
    assert_eq!(
        route.expected_ticks(config).expect("continuous ticks"),
        666_667
    );
}

#[test]
fn route_helpers_and_contextual_failures_cover_bounded_error_paths() {
    let route = plan_fixed_repeated_route_with(&StaticLeg::open(), TileCoord::new(10, 10))
        .expect("preferred route");
    assert_eq!(route.waypoints(), [route.start, route.alternate]);
    assert_eq!(route.movement_waypoints().expect("waypoints").len(), 25_000);

    let config = WorldConfig::new(64, 64, Seed(9)).expect("config");
    let mut zero_speed = config;
    zero_speed.move_speed_subunits_per_tick = 0;
    assert!(matches!(
        route.expected_ticks(zero_speed),
        Err(SourceQualificationError::FixedRouteTickBound {
            required: u64::MAX,
            maximum: 0
        })
    ));
    let mut zero_denominator = config;
    zero_denominator.move_speed_subunits_per_tick_denominator = 0;
    assert!(matches!(
        route.expected_ticks(zero_denominator),
        Err(SourceQualificationError::FixedRouteTickBound {
            required: u64::MAX,
            maximum: 0
        })
    ));
    let mut overflowing = route;
    overflowing.repetitions = u64::MAX;
    assert!(matches!(
        overflowing.expected_ticks(config),
        Err(SourceQualificationError::FixedRouteTickBound {
            required: u64::MAX,
            maximum: 0
        })
    ));

    let generator = MapChunkGenerator::new([0; 32], 1, 64);
    let origin = TileCoord::new(10, 10);
    let destination = TileCoord::new(12, 10);
    assert!(matches!(
        contextual_route_failure(
            GameWorldError::InvalidPosition,
            &generator,
            origin,
            destination
        ),
        SourceQualificationError::ImpassableWaypoint { x: 12, y: 10 }
    ));
    let unreachable =
        contextual_route_failure(GameWorldError::Unreachable, &generator, origin, destination);
    assert!(matches!(
        unreachable,
        SourceQualificationError::UnreachableWaypoint { x: 12, y: 10, .. }
    ));
    assert!(matches!(
        contextual_route_failure(
            GameWorldError::UnknownEntity,
            &generator,
            origin,
            destination
        ),
        SourceQualificationError::Movement(GameWorldError::UnknownEntity)
    ));
    assert!(
        destination_diagnostic(&generator, destination)
            .expect("destination detail")
            .contains("destination_material=")
    );
    assert!(matches!(
        destination_diagnostic(&generator, TileCoord::new(64, 64)),
        Err(SourceQualificationError::Page(_))
    ));
    assert_eq!(
        WorldPosition::from_tile_center(destination)
            .expect("destination position")
            .tile_floor(),
        destination
    );
}
