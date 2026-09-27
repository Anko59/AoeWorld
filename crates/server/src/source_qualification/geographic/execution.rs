use super::{
    GeographicMovementResult, HASH_CHECK_INTERVAL, LONG_ORDER_OFFSETS, METERS_PER_TILE,
    REQUIRED_DISTANCE_METERS, ensure_replay, hex,
};
use crate::PageResidency;
use crate::source_qualification::{
    SourceQualificationError, SourceQualificationProgress,
    metrics::{NavigationCachePeaks, ProcessRssSampler},
    route::contextual_route_failure,
};
use aoe_core::{FIXED_SUBUNITS_PER_TILE, PlayerId, TileCoord, WorldPosition};
use aoe_map::{EnvironmentPageProvider, MapPackage};
use aoe_simulation::{GameWorld, GameWorldError};
use std::{collections::BTreeSet, path::Path, sync::Arc};

pub(super) fn execute(
    package_directory: &Path,
    package: &MapPackage,
    start: TileCoord,
    route_waypoints: &[TileCoord],
    max_ticks: u64,
    progress: &mut dyn FnMut(SourceQualificationProgress),
    rss: &mut ProcessRssSampler,
) -> Result<GeographicMovementResult, SourceQualificationError> {
    let movement_provider = PageResidency::open(package_directory, package, &|| false)?;
    let replay_provider = PageResidency::open(package_directory, package, &|| false)?;
    let mut world = GameWorld::from_page_provider(
        package.clone(),
        movement_provider.clone() as Arc<dyn EnvironmentPageProvider>,
    )?;
    let mut replay = GameWorld::from_page_provider(
        package.clone(),
        replay_provider.clone() as Arc<dyn EnvironmentPageProvider>,
    )?;
    let unit = world.spawn_unit(PlayerId(0), WorldPosition::from_tile_center(start)?)?;
    let replay_unit = replay.spawn_unit(PlayerId(0), WorldPosition::from_tile_center(start)?)?;
    let generator = package.generator_with_page_provider(
        movement_provider.clone() as Arc<dyn EnvironmentPageProvider>
    )?;
    let route_loads_before = movement_provider.verified_page_loads();
    let replay_loads_before = replay_provider.verified_page_loads();
    let config = world.config();

    let mut navigation_cache_peaks = NavigationCachePeaks::default();
    let mut peak_resident_pages = movement_provider
        .resident_pages()
        .max(replay_provider.resident_pages());
    let mut moved_subunits = 0.0_f64;
    let mut replay_moved_subunits = 0.0_f64;
    let mut movement_ticks = 0_u64;
    let mut completed_orders = 0_usize;
    let mut route_checkpoint_count = 0_usize;
    let mut comparison_count = 0_usize;
    let mut last_position = world
        .unit(unit)
        .ok_or(GameWorldError::UnknownEntity)?
        .position;
    let mut replay_last_position = replay
        .unit(replay_unit)
        .ok_or(GameWorldError::UnknownEntity)?
        .position;
    for (leg_index, destination) in route_waypoints.iter().copied().enumerate() {
        let target = WorldPosition::from_tile_center(destination)?;
        for (world, entity) in [(&mut world, unit), (&mut replay, replay_unit)] {
            world
                .issue_move(entity, target)
                .map_err(|error| contextual_route_failure(error, &generator, start, destination))?;
        }
        loop {
            if movement_ticks >= max_ticks {
                return Err(SourceQualificationError::TickLimit);
            }
            let route_order_before = world.movement_order(unit);
            let replay_order_before = replay.movement_order(replay_unit);
            world.advance();
            replay.advance();
            movement_ticks += 1;
            let state = world.unit(unit).ok_or(GameWorldError::UnknownEntity)?;
            let replay_state = replay
                .unit(replay_unit)
                .ok_or(GameWorldError::UnknownEntity)?;
            let route_order_after = world.movement_order(unit);
            let replay_order_after = replay.movement_order(replay_unit);
            moved_subunits += super::super::movement::advanced_distance(
                last_position,
                route_order_before,
                state.position,
                route_order_after,
            );
            replay_moved_subunits += super::super::movement::advanced_distance(
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
                return Err(contextual_route_failure(
                    error,
                    &generator,
                    state.position.tile_floor(),
                    destination,
                ));
            }
            if let Some(error) = replay.movement_failure(replay_unit) {
                return Err(contextual_route_failure(
                    error,
                    &generator,
                    replay_state.position.tile_floor(),
                    destination,
                ));
            }
            if movement_ticks.is_multiple_of(HASH_CHECK_INTERVAL) {
                rss.observe();
                comparison_count += 1;
                if state != replay_state {
                    return Err(SourceQualificationError::ReplayDiverged(movement_ticks));
                }
                ensure_replay(
                    world.canonical_hash(),
                    replay.canonical_hash(),
                    movement_ticks,
                )?;
                route_checkpoint_count += 1;
            }
            if route_order_after.is_none() && replay_order_after.is_none() {
                if state.position != target || replay_state.position != target {
                    let observed = if state.position == target {
                        replay_state.position
                    } else {
                        state.position
                    };
                    return Err(SourceQualificationError::MovementEndpointMismatch {
                        expected: [destination.x, destination.y],
                        observed: [observed.tile_floor().x, observed.tile_floor().y],
                    });
                }
                ensure_replay(
                    world.canonical_hash(),
                    replay.canonical_hash(),
                    movement_ticks,
                )?;
                comparison_count += 1;
                completed_orders += 1;
                break;
            }
            if movement_ticks.is_multiple_of(50_000) {
                progress(SourceQualificationProgress {
                    tick: movement_ticks,
                    leg: leg_index + 1,
                    moved_meters: moved_subunits / f64::from(FIXED_SUBUNITS_PER_TILE)
                        * METERS_PER_TILE,
                });
            }
        }
    }
    let route_hash = world.canonical_hash();
    let replay_hash = replay.canonical_hash();
    ensure_replay(route_hash, replay_hash, movement_ticks)?;
    let measured_distance_meters =
        moved_subunits / f64::from(FIXED_SUBUNITS_PER_TILE) * METERS_PER_TILE;
    let replay_distance_meters =
        replay_moved_subunits / f64::from(FIXED_SUBUNITS_PER_TILE) * METERS_PER_TILE;
    if measured_distance_meters < REQUIRED_DISTANCE_METERS {
        return Err(SourceQualificationError::MovementDistanceMismatch {
            expected: REQUIRED_DISTANCE_METERS,
            observed: measured_distance_meters,
        });
    }
    if replay_distance_meters != measured_distance_meters {
        return Err(SourceQualificationError::ReplayDistanceMismatch {
            route_meters: measured_distance_meters,
            replay_meters: replay_distance_meters,
        });
    }
    let simulated_seconds = movement_ticks as f64 / f64::from(config.tick_hz);
    let measured_speed_meters_per_second = measured_distance_meters / simulated_seconds;
    let configured_speed_meters_per_second = f64::from(aoe_map::CAVALRY_METERS_PER_SECOND);
    let speed_within_one_percent =
        (measured_speed_meters_per_second - configured_speed_meters_per_second).abs()
            <= configured_speed_meters_per_second * 0.01;
    if !speed_within_one_percent {
        return Err(SourceQualificationError::PhysicalSpeedMismatch {
            configured: configured_speed_meters_per_second,
            observed: measured_speed_meters_per_second,
        });
    }
    if completed_orders != LONG_ORDER_OFFSETS.len() {
        return Err(SourceQualificationError::ReportFieldMismatch {
            field: "geographic_navigation.completed_orders",
            left: completed_orders.to_string(),
            right: LONG_ORDER_OFFSETS.len().to_string(),
        });
    }

    let mut min_x = start.x;
    let mut max_x = start.x;
    let mut min_y = start.y;
    let mut max_y = start.y;
    for tile in route_waypoints {
        min_x = min_x.min(tile.x);
        max_x = max_x.max(tile.x);
        min_y = min_y.min(tile.y);
        max_y = max_y.max(tile.y);
    }
    let unique_corridor_count = std::iter::once(start)
        .chain(route_waypoints.iter().copied())
        .zip(route_waypoints.iter().copied())
        .map(|(from, to)| {
            if (from.x, from.y) <= (to.x, to.y) {
                ((from.x, from.y), (to.x, to.y))
            } else {
                ((to.x, to.y), (from.x, from.y))
            }
        })
        .collect::<BTreeSet<_>>()
        .len();
    rss.observe();

    Ok(GeographicMovementResult {
        completed_orders,
        measured_distance_meters,
        movement_ticks,
        simulated_seconds,
        measured_speed_meters_per_second,
        configured_speed_meters_per_second,
        speed_within_one_percent,
        spatial_extent_tiles: [max_x - min_x, max_y - min_y],
        peak_route_navigation_cache_entries: navigation_cache_peaks.route_entries,
        peak_replay_navigation_cache_entries: navigation_cache_peaks.replay_entries,
        peak_combined_navigation_cache_entries: navigation_cache_peaks.combined_entries,
        peak_combined_navigation_cache_logical_retained_bytes: navigation_cache_peaks
            .combined_logical_retained_bytes,
        unique_corridor_count,
        route_page_loads: movement_provider
            .verified_page_loads()
            .saturating_sub(route_loads_before),
        replay_page_loads: replay_provider
            .verified_page_loads()
            .saturating_sub(replay_loads_before),
        peak_resident_pages_per_provider: peak_resident_pages
            .max(movement_provider.resident_pages())
            .max(replay_provider.resident_pages()),
        route_checkpoint_count,
        replay_hash: hex(&route_hash),
        replay_matches: route_hash == replay_hash && comparison_count > 0,
    })
}
