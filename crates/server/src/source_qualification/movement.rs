use super::{
    SourceQualificationError, SourceQualificationProgress,
    metrics::{NavigationCachePeaks, ProcessRssSampler},
    route::{FixedRoute, contextual_route_failure},
};
use crate::PageResidency;
use aoe_core::{PlayerId, TileCoord, WorldPosition};
use aoe_map::MapPackage;
use aoe_simulation::{GameWorld, GameWorldError};

const HASH_CHECK_INTERVAL: u64 = 8_192;

pub(super) struct MovementRun {
    pub(super) start_tile: TileCoord,
    pub(super) route: FixedRoute,
    pub(super) movement_ticks: u64,
    pub(super) route_moved_meters: f64,
    pub(super) replay_moved_meters: f64,
    pub(super) route_checkpoint_count: usize,
    pub(super) movement_replay_comparison_count: usize,
    pub(super) route_movement_leg_count: u64,
    pub(super) replay_movement_leg_count: u64,
    pub(super) peak_resident_pages: usize,
    pub(super) navigation_cache_peaks: NavigationCachePeaks,
    pub(super) route_hash: [u8; 32],
    pub(super) replay_hash: [u8; 32],
}

impl MovementRun {
    fn validate(&self, config: aoe_core::WorldConfig) -> Result<(), SourceQualificationError> {
        let expected_ticks = self.route.expected_ticks(config)?;
        if self.movement_ticks != expected_ticks {
            return Err(SourceQualificationError::MovementTickMismatch {
                expected: expected_ticks,
                observed: self.movement_ticks,
            });
        }
        for observed in [
            self.route.repetitions,
            self.route_movement_leg_count,
            self.replay_movement_leg_count,
        ] {
            if observed != super::route::REQUIRED_REPETITIONS {
                return Err(SourceQualificationError::MovementRepetitionMismatch {
                    expected: super::route::REQUIRED_REPETITIONS,
                    observed,
                });
            }
        }
        validate_distance(
            self.route,
            self.route_moved_meters,
            self.replay_moved_meters,
        )?;
        ensure_replay(self.route_hash, self.replay_hash, self.movement_ticks)
    }
}

pub(super) struct MovementInput<'a> {
    pub(super) package: &'a MapPackage,
    pub(super) generator: &'a aoe_map::MapChunkGenerator,
    pub(super) movement_provider: std::sync::Arc<PageResidency>,
    pub(super) replay_provider: std::sync::Arc<PageResidency>,
    pub(super) initial_peak_resident_pages: usize,
    pub(super) max_ticks: u64,
    pub(super) route: FixedRoute,
    pub(super) progress: &'a mut dyn FnMut(SourceQualificationProgress),
    pub(super) rss: &'a mut ProcessRssSampler,
}

pub(super) fn exercise_movement(
    input: MovementInput<'_>,
) -> Result<MovementRun, SourceQualificationError> {
    let MovementInput {
        package,
        generator,
        movement_provider,
        replay_provider,
        initial_peak_resident_pages,
        max_ticks,
        route,
        progress,
        rss,
    } = input;
    let mut world = GameWorld::from_page_provider(
        package.clone(),
        movement_provider.clone() as std::sync::Arc<dyn aoe_map::EnvironmentPageProvider>,
    )?;
    let mut replay = GameWorld::from_page_provider(
        package.clone(),
        replay_provider.clone() as std::sync::Arc<dyn aoe_map::EnvironmentPageProvider>,
    )?;
    route.ensure_tick_bound(world.config(), max_ticks)?;
    let unit = world.spawn_unit(PlayerId(0), WorldPosition::from_tile_center(route.start)?)?;
    let replay_unit =
        replay.spawn_unit(PlayerId(0), WorldPosition::from_tile_center(route.start)?)?;
    let mut navigation_cache_peaks = NavigationCachePeaks::default();
    let mut movement_replay_comparison_count = 0_usize;
    let mut route_movement_leg_count = 0_u64;
    let mut replay_movement_leg_count = 0_u64;
    let waypoints = route.movement_waypoints()?;
    let issue_route = |world: &mut GameWorld, id| {
        world
            .issue_move_waypoints(id, &waypoints)
            .map(|_| ())
            .map_err(|error| {
                contextual_route_failure(
                    error,
                    generator,
                    route.start,
                    route.destination_for_leg(route.repetitions - 1),
                )
            })
    };
    issue_route(&mut world, unit)?;
    issue_route(&mut replay, replay_unit)?;
    let expected_queue = waypoints.len().saturating_sub(1);
    for (world, id) in [(&world, unit), (&replay, replay_unit)] {
        let queued = world
            .queued_waypoint_count(id)
            .ok_or(GameWorldError::UnknownEntity)?;
        if queued != expected_queue {
            return Err(SourceQualificationError::MovementQueueMismatch {
                expected: expected_queue,
                observed: queued,
            });
        }
    }
    navigation_cache_peaks.observe(&world, &replay);

    let mut last_position = world
        .unit(unit)
        .ok_or(GameWorldError::UnknownEntity)?
        .position;
    let mut replay_last_position = replay
        .unit(replay_unit)
        .ok_or(GameWorldError::UnknownEntity)?
        .position;
    let mut moved_subunits = 0.0_f64;
    let mut replay_moved_subunits = 0.0_f64;
    let mut route_checkpoint_count = 0_usize;
    let mut movement_ticks = 0_u64;
    let mut peak_resident_pages = initial_peak_resident_pages
        .max(movement_provider.resident_pages())
        .max(replay_provider.resident_pages());
    for tick in 1..=max_ticks {
        let route_order_before = world.movement_order(unit);
        let replay_order_before = replay.movement_order(replay_unit);
        world.advance();
        replay.advance();
        movement_ticks = tick;
        let state = world.unit(unit).ok_or(GameWorldError::UnknownEntity)?;
        let replay_state = replay
            .unit(replay_unit)
            .ok_or(GameWorldError::UnknownEntity)?;
        let route_order_after = world.movement_order(unit);
        let replay_order_after = replay.movement_order(replay_unit);
        moved_subunits += advanced_distance(
            last_position,
            route_order_before,
            state.position,
            route_order_after,
        );
        replay_moved_subunits += advanced_distance(
            replay_last_position,
            replay_order_before,
            replay_state.position,
            replay_order_after,
        );
        last_position = state.position;
        replay_last_position = replay_state.position;
        peak_resident_pages = peak_resident_pages
            .max(movement_provider.resident_pages())
            .max(replay_provider.resident_pages());
        navigation_cache_peaks.observe(&world, &replay);
        if let Some(error) = world.movement_failure(unit) {
            let destination = route.destination_for_leg(route.repetitions - 1);
            let origin = state.position.tile_floor();
            return Err(contextual_route_failure(
                error,
                generator,
                origin,
                destination,
            ));
        }
        if let Some(error) = replay.movement_failure(replay_unit) {
            let destination = route.destination_for_leg(route.repetitions - 1);
            let origin = replay_state.position.tile_floor();
            return Err(contextual_route_failure(
                error,
                generator,
                origin,
                destination,
            ));
        }
        if tick % HASH_CHECK_INTERVAL == 0 {
            rss.observe();
            movement_replay_comparison_count += 1;
            if state != replay_state {
                return Err(SourceQualificationError::ReplayDiverged(tick));
            }
            ensure_replay(world.canonical_hash(), replay.canonical_hash(), tick)?;
            route_checkpoint_count += 1;
        }
        if world.movement_order(unit).is_none() && replay.movement_order(replay_unit).is_none() {
            for (world, id) in [(&world, unit), (&replay, replay_unit)] {
                let queued = world
                    .queued_waypoint_count(id)
                    .ok_or(GameWorldError::UnknownEntity)?;
                if queued != 0 {
                    return Err(SourceQualificationError::MovementQueueMismatch {
                        expected: 0,
                        observed: queued,
                    });
                }
            }
            let expected_endpoint = WorldPosition::from_tile_center(route.start)?;
            if state.position != expected_endpoint || replay_state.position != expected_endpoint {
                let observed = if state.position == expected_endpoint {
                    replay_state.position
                } else {
                    state.position
                };
                return Err(SourceQualificationError::MovementEndpointMismatch {
                    expected: [route.start.x, route.start.y],
                    observed: [observed.tile_floor().x, observed.tile_floor().y],
                });
            }
            route_movement_leg_count = route.repetitions;
            replay_movement_leg_count = route.repetitions;
            movement_replay_comparison_count += 1;
            if state != replay_state {
                return Err(SourceQualificationError::ReplayDiverged(tick));
            }
            ensure_replay(world.canonical_hash(), replay.canonical_hash(), tick)?;
            break;
        }
        if tick % 50_000 == 0 {
            let moved_meters = moved_subunits / 1_024.0 * 2.0;
            let completed_legs = (moved_meters / route.leg_length_meters).floor();
            progress(SourceQualificationProgress {
                tick,
                leg: (completed_legs as u64)
                    .saturating_add(1)
                    .min(route.repetitions) as usize,
                moved_meters,
            });
        }
        if tick == max_ticks {
            return Err(SourceQualificationError::TickLimit);
        }
    }
    movement_replay_comparison_count += 1;
    let route_hash = world.canonical_hash();
    let replay_hash = replay.canonical_hash();
    ensure_replay(route_hash, replay_hash, movement_ticks)?;
    let route_moved_meters = moved_subunits / 1_024.0 * 2.0;
    let replay_moved_meters = replay_moved_subunits / 1_024.0 * 2.0;
    navigation_cache_peaks.observe(&world, &replay);
    peak_resident_pages = peak_resident_pages
        .max(movement_provider.resident_pages())
        .max(replay_provider.resident_pages());
    rss.observe();
    let run = MovementRun {
        start_tile: route.start,
        route,
        movement_ticks,
        route_moved_meters,
        replay_moved_meters,
        route_checkpoint_count,
        movement_replay_comparison_count,
        route_movement_leg_count,
        replay_movement_leg_count,
        peak_resident_pages,
        navigation_cache_peaks,
        route_hash,
        replay_hash,
    };
    run.validate(world.config())?;
    Ok(run)
}

fn ensure_replay(
    route_hash: [u8; 32],
    replay_hash: [u8; 32],
    tick: u64,
) -> Result<(), SourceQualificationError> {
    if route_hash != replay_hash {
        return Err(SourceQualificationError::ReplayDiverged(tick));
    }
    Ok(())
}

fn validate_distance(
    route: FixedRoute,
    route_meters: f64,
    replay_meters: f64,
) -> Result<(), SourceQualificationError> {
    let expected = super::route::REQUIRED_TRAVEL_METERS;
    if route.accumulated_distance_meters(route.repetitions) != expected {
        return Err(SourceQualificationError::MovementDistanceMismatch {
            expected,
            observed: route.accumulated_distance_meters(route.repetitions),
        });
    }
    if route_meters != expected {
        return Err(SourceQualificationError::MovementDistanceMismatch {
            expected,
            observed: route_meters,
        });
    }
    if replay_meters != route_meters {
        return Err(SourceQualificationError::ReplayDistanceMismatch {
            route_meters,
            replay_meters,
        });
    }
    Ok(())
}

fn displacement(from: WorldPosition, to: WorldPosition) -> f64 {
    let dx = f64::from(to.x) - f64::from(from.x);
    let dy = f64::from(to.y) - f64::from(from.y);
    dx.hypot(dy)
}

pub(super) fn advanced_distance(
    before: WorldPosition,
    order_before: Option<aoe_simulation::MovementOrder>,
    after: WorldPosition,
    order_after: Option<aoe_simulation::MovementOrder>,
) -> f64 {
    let Some(order_before) = order_before else {
        return displacement(before, after);
    };
    let Some(order_after) = order_after else {
        return displacement(before, after);
    };
    if order_before.origin == order_after.origin && order_before.waypoint == order_after.waypoint {
        return displacement(before, after);
    }
    displacement(before, order_before.waypoint) + displacement(order_before.waypoint, after)
}

#[cfg(test)]
mod tests;
