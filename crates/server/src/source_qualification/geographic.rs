//! Source-backed long-order qualification on a separately selected region.

use super::{
    MAX_ROUTE_TICKS, SourceQualificationError, SourceQualificationProgress,
    metrics::{NavigationCachePeaks, ProcessRssSampler},
    report::{GeographicNavigationEvidence, RoutePlanningDiagnostic, RoutePlanningOutcome},
    route::contextual_route_failure,
};
use crate::PageResidency;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, PlayerId, TileCoord, WorldPosition};
use aoe_map::{EnvironmentPageProvider, LayerProvenance, MapPackage, RoutePlannerPoll};
use aoe_simulation::{GameWorld, GameWorldError, StartSearchResult};
use std::{collections::BTreeSet, path::Path, sync::Arc};

const ROUTE_CONTRACT: &str = "five-ordinary-20km-orders-100km-total-source-backed-v1";
const LONG_ORDER_OFFSETS: [(i32, i32); 5] = [(10_000, 0), (0, 0), (10_000, 0), (0, 0), (10_000, 0)];
const METERS_PER_TILE: f64 = aoe_map::GAME_TILE_METERS as f64;
const REQUIRED_DISTANCE_METERS: f64 = 100_000.0;
const LONG_ORDER_DISTANCE_METERS: f64 = 20_000.0;
const HASH_CHECK_INTERVAL: u64 = 8_192;
const REFERENCE_CENTER_E7: (i32, i32) = (250_000_000, -50_000_000);

pub(super) fn qualify(
    package_directory: &Path,
    content_hash: &str,
    max_ticks: u64,
    progress: &mut dyn FnMut(SourceQualificationProgress),
    rss: &mut ProcessRssSampler,
) -> Result<GeographicNavigationEvidence, SourceQualificationError> {
    if max_ticks == 0 || max_ticks > MAX_ROUTE_TICKS {
        return Err(SourceQualificationError::TickLimit);
    }
    let package = load_reference_package(package_directory, content_hash)?;
    let planner_provider = PageResidency::open(package_directory, &package, &|| false)?;
    let planner_world = GameWorld::from_page_provider(
        package.clone(),
        planner_provider.clone() as Arc<dyn EnvironmentPageProvider>,
    )?;
    let config = planner_world.config();
    let start = match planner_world.terrain().search_start_for_recipe(
        config,
        package.generation_recipe_version,
        64,
        || false,
    )? {
        StartSearchResult::Found(tile) => tile,
        StartSearchResult::LimitReached => {
            return Err(SourceQualificationError::GeographicStartSearchLimit);
        }
        StartSearchResult::Unavailable => return Err(SourceQualificationError::NoStart),
        StartSearchResult::Cancelled => return Err(SourceQualificationError::GeographicCancelled),
    };
    let route_waypoints = planned_waypoints(start)?;
    let north_origin = start;
    let north_destination = TileCoord::new(start.x, start.y.saturating_add(10_000));
    let north_flat_route = plan_order(
        planner_world.terrain(),
        north_origin,
        north_destination,
        "sahara_reference_northbound_search_limit_regression_v1",
        None,
    )?;
    let north_connectivity = super::diagnostics::bounded_connectivity_diagnostic(
        planner_world.terrain(),
        config,
        north_origin,
        north_destination,
    )?;
    let route_planning_diagnostics = route_waypoints
        .iter()
        .enumerate()
        .scan(start, |origin, (index, destination)| {
            let prior = *origin;
            *origin = *destination;
            Some((index, prior, *destination))
        })
        .map(|(index, origin, destination)| {
            let diagnostic = plan_order(
                planner_world.terrain(),
                origin,
                destination,
                "sahara_reference_long_order_chain_v1",
                Some(index as u8 + 1),
            )?;
            if diagnostic.outcome != RoutePlanningOutcome::Complete {
                return Err(SourceQualificationError::GeographicRoutePlanningFailed {
                    origin: diagnostic.origin,
                    destination: diagnostic.destination,
                    outcome: diagnostic.outcome,
                    work: diagnostic.work,
                    expansions: diagnostic.expansions,
                });
            }
            Ok(diagnostic)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let route_planner_work_total = route_planning_diagnostics
        .iter()
        .map(|diagnostic| u64::from(diagnostic.work))
        .sum();
    let route_planner_work_max_order = route_planning_diagnostics
        .iter()
        .map(|diagnostic| diagnostic.work)
        .max()
        .unwrap_or_default();
    let route_planner_expansions_total = route_planning_diagnostics
        .iter()
        .map(|diagnostic| u64::from(diagnostic.expansions))
        .sum();
    let route_planner_peak_retained_entries = route_planning_diagnostics
        .iter()
        .map(|diagnostic| diagnostic.peak_retained_entries)
        .max()
        .unwrap_or_default();

    let movement_provider = PageResidency::open(package_directory, &package, &|| false)?;
    let replay_provider = PageResidency::open(package_directory, &package, &|| false)?;
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
        .ok_or(aoe_simulation::GameWorldError::UnknownEntity)?
        .position;
    let mut replay_last_position = replay
        .unit(replay_unit)
        .ok_or(aoe_simulation::GameWorldError::UnknownEntity)?
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
            let state = world
                .unit(unit)
                .ok_or(aoe_simulation::GameWorldError::UnknownEntity)?;
            let replay_state = replay
                .unit(replay_unit)
                .ok_or(aoe_simulation::GameWorldError::UnknownEntity)?;
            let route_order_after = world.movement_order(unit);
            let replay_order_after = replay.movement_order(replay_unit);
            moved_subunits += super::movement::advanced_distance(
                last_position,
                route_order_before,
                state.position,
                route_order_after,
            );
            replay_moved_subunits += super::movement::advanced_distance(
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
    for tile in &route_waypoints {
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
    Ok(GeographicNavigationEvidence {
        contract: ROUTE_CONTRACT,
        package_hash: package.content_hash_hex(),
        map_center_latitude_e7: package.request.center_latitude_e7,
        map_center_longitude_e7: package.request.center_longitude_e7,
        start_tile: [start.x, start.y],
        route_waypoints: route_waypoints
            .iter()
            .map(|tile| [tile.x, tile.y])
            .collect(),
        long_order_count: completed_orders,
        planned_distance_meters: REQUIRED_DISTANCE_METERS,
        measured_distance_meters,
        maximum_order_displacement_meters: LONG_ORDER_DISTANCE_METERS,
        spatial_extent_tiles: [max_x - min_x, max_y - min_y],
        movement_ticks,
        simulated_seconds,
        measured_speed_meters_per_second,
        configured_speed_meters_per_second,
        speed_within_one_percent,
        route_planner_work_total,
        route_planner_work_max_order,
        route_planner_expansions_total,
        route_planner_peak_retained_entries,
        peak_route_navigation_cache_entries: navigation_cache_peaks.route_entries,
        peak_replay_navigation_cache_entries: navigation_cache_peaks.replay_entries,
        peak_combined_navigation_cache_entries: navigation_cache_peaks.combined_entries,
        peak_combined_navigation_cache_logical_retained_bytes: navigation_cache_peaks
            .combined_logical_retained_bytes,
        route_planning_diagnostics,
        north_flat_route_diagnostic: north_flat_route,
        north_connectivity_diagnostic: north_connectivity,
        route_pattern: "five alternating ordinary orders over one 20km corridor; four repeat traversals",
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

fn load_reference_package(
    package_directory: &Path,
    content_hash: &str,
) -> Result<MapPackage, SourceQualificationError> {
    let packages = crate::load_map_packages(Some(package_directory))?;
    let package = packages.get(content_hash).cloned().ok_or_else(|| {
        SourceQualificationError::UnknownPackage {
            requested: content_hash.to_owned(),
            available: packages.keys().cloned().collect(),
        }
    })?;
    package
        .validate()
        .map_err(|_| SourceQualificationError::UnsupportedPackage)?;
    if !super::supported_recipe(package.generation_recipe_version) {
        return Err(SourceQualificationError::UnsupportedGenerationRecipe(
            package.generation_recipe_version,
        ));
    }
    if package.source_locks.is_empty()
        || package.request.compression.numerator != 1
        || package.request.compression.denominator != 1
        || package.estimate.tiles_per_side != 50_000
        || package.estimate.game_side_meters != 100_000
        || package.environment.samples_per_axis == 0
        || package.provenance.elevation != LayerProvenance::SourceDerived
    {
        return Err(SourceQualificationError::UnsupportedPackage);
    }
    if (
        package.request.center_latitude_e7,
        package.request.center_longitude_e7,
    ) != REFERENCE_CENTER_E7
    {
        return Err(
            SourceQualificationError::GeographicReferenceLocationMismatch {
                latitude_e7: package.request.center_latitude_e7,
                longitude_e7: package.request.center_longitude_e7,
            },
        );
    }
    Ok(package)
}

fn planned_waypoints(start: TileCoord) -> Result<Vec<TileCoord>, SourceQualificationError> {
    LONG_ORDER_OFFSETS
        .iter()
        .map(|(dx, dy)| {
            let tile = TileCoord::new(start.x.saturating_add(*dx), start.y.saturating_add(*dy));
            (tile.x >= 0 && tile.y >= 0 && tile.x < 50_000 && tile.y < 50_000)
                .then_some(tile)
                .ok_or(SourceQualificationError::GeographicWaypointOutsideMap {
                    x: tile.x,
                    y: tile.y,
                })
        })
        .collect()
}

pub(super) fn plan_order(
    terrain: &aoe_simulation::Terrain,
    origin: TileCoord,
    destination: TileCoord,
    case: &'static str,
    chain_leg: Option<u8>,
) -> Result<RoutePlanningDiagnostic, SourceQualificationError> {
    let mut planner = terrain
        .route_planner(origin, destination, aoe_map::MAX_ROUTE_PLANNER_WORK)
        .ok_or(SourceQualificationError::Movement(
            GameWorldError::InvalidTerrain,
        ))?;
    let mut path_tile_count = 0_usize;
    let outcome = loop {
        match terrain
            .poll_route_planner(&mut planner, 16_384, &|| false)
            .ok_or(SourceQualificationError::Movement(
                GameWorldError::InvalidTerrain,
            ))? {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                path_tile_count = path_tile_count.saturating_add(path.tiles.len())
            }
            RoutePlannerPoll::Complete => break RoutePlanningOutcome::Complete,
            RoutePlannerPoll::InvalidDestination => break RoutePlanningOutcome::InvalidDestination,
            RoutePlannerPoll::Unreachable => break RoutePlanningOutcome::Unreachable,
            RoutePlannerPoll::SearchLimit => break RoutePlanningOutcome::SearchLimit,
            RoutePlannerPoll::Environment(error) => match error {
                aoe_map::EnvironmentPageError::Cancelled => break RoutePlanningOutcome::Cancelled,
                _ => break RoutePlanningOutcome::ProviderError,
            },
        }
    };
    Ok(RoutePlanningDiagnostic {
        case,
        chain_leg,
        origin: [origin.x, origin.y],
        destination: [destination.x, destination.y],
        outcome,
        work: planner.work(),
        expansions: planner.expansions(),
        peak_retained_entries: planner.peak_retained_entries(),
        path_tile_count,
        work_limit: aoe_map::MAX_ROUTE_PLANNER_WORK,
        node_limit: aoe_map::MAX_ROUTE_PLANNER_NODES,
    })
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

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests;
