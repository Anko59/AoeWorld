//! Source-backed long-order qualification on a separately selected region.

mod execution;

use super::{
    MAX_ROUTE_TICKS, SourceQualificationError, SourceQualificationProgress,
    metrics::ProcessRssSampler,
    report::{GeographicNavigationEvidence, RoutePlanningDiagnostic, RoutePlanningOutcome},
};
use crate::PageResidency;
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageProvider, LayerProvenance, MapPackage, RoutePlannerPoll};
use aoe_simulation::{GameWorld, GameWorldError, StartSearchResult};
use std::{path::Path, sync::Arc};

const ROUTE_CONTRACT: &str = "five-ordinary-20km-orders-100km-total-source-backed-v1";
const LONG_ORDER_OFFSETS: [(i32, i32); 5] = [(10_000, 0), (0, 0), (10_000, 0), (0, 0), (10_000, 0)];
const METERS_PER_TILE: f64 = aoe_map::GAME_TILE_METERS as f64;
const REQUIRED_DISTANCE_METERS: f64 = 100_000.0;
const LONG_ORDER_DISTANCE_METERS: f64 = 20_000.0;
const HASH_CHECK_INTERVAL: u64 = 8_192;
const REFERENCE_CENTER_E7: (i32, i32) = (250_000_000, -50_000_000);

#[derive(Debug)]
pub(super) struct GeographicMovementResult {
    pub(super) completed_orders: usize,
    pub(super) measured_distance_meters: f64,
    pub(super) movement_ticks: u64,
    pub(super) simulated_seconds: f64,
    pub(super) measured_speed_meters_per_second: f64,
    pub(super) configured_speed_meters_per_second: f64,
    pub(super) speed_within_one_percent: bool,
    pub(super) spatial_extent_tiles: [i32; 2],
    pub(super) peak_route_navigation_cache_entries: usize,
    pub(super) peak_replay_navigation_cache_entries: usize,
    pub(super) peak_combined_navigation_cache_entries: usize,
    pub(super) peak_combined_navigation_cache_logical_retained_bytes: usize,
    pub(super) unique_corridor_count: usize,
    pub(super) route_page_loads: u64,
    pub(super) replay_page_loads: u64,
    pub(super) peak_resident_pages_per_provider: usize,
    pub(super) route_checkpoint_count: usize,
    pub(super) replay_hash: String,
    pub(super) replay_matches: bool,
}

pub(super) fn execute_movement(
    package_directory: &Path,
    package: &MapPackage,
    start: TileCoord,
    route_waypoints: &[TileCoord],
    max_ticks: u64,
    progress: &mut dyn FnMut(SourceQualificationProgress),
    rss: &mut ProcessRssSampler,
) -> Result<GeographicMovementResult, SourceQualificationError> {
    execution::execute(
        package_directory,
        package,
        start,
        route_waypoints,
        max_ticks,
        progress,
        rss,
    )
}

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

    let movement = execute_movement(
        package_directory,
        &package,
        start,
        &route_waypoints,
        max_ticks,
        progress,
        rss,
    )?;
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
        long_order_count: movement.completed_orders,
        planned_distance_meters: REQUIRED_DISTANCE_METERS,
        measured_distance_meters: movement.measured_distance_meters,
        maximum_order_displacement_meters: LONG_ORDER_DISTANCE_METERS,
        spatial_extent_tiles: movement.spatial_extent_tiles,
        movement_ticks: movement.movement_ticks,
        simulated_seconds: movement.simulated_seconds,
        measured_speed_meters_per_second: movement.measured_speed_meters_per_second,
        configured_speed_meters_per_second: movement.configured_speed_meters_per_second,
        speed_within_one_percent: movement.speed_within_one_percent,
        route_planner_work_total,
        route_planner_work_max_order,
        route_planner_expansions_total,
        route_planner_peak_retained_entries,
        peak_route_navigation_cache_entries: movement.peak_route_navigation_cache_entries,
        peak_replay_navigation_cache_entries: movement.peak_replay_navigation_cache_entries,
        peak_combined_navigation_cache_entries: movement.peak_combined_navigation_cache_entries,
        peak_combined_navigation_cache_logical_retained_bytes: movement
            .peak_combined_navigation_cache_logical_retained_bytes,
        route_planning_diagnostics,
        north_flat_route_diagnostic: north_flat_route,
        north_connectivity_diagnostic: north_connectivity,
        route_pattern: "five alternating ordinary orders over one 20km corridor; four repeat traversals",
        unique_corridor_count: movement.unique_corridor_count,
        route_page_loads: movement.route_page_loads,
        replay_page_loads: movement.replay_page_loads,
        peak_resident_pages_per_provider: movement.peak_resident_pages_per_provider,
        route_checkpoint_count: movement.route_checkpoint_count,
        replay_hash: movement.replay_hash,
        replay_matches: movement.replay_matches,
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

pub(super) fn planned_waypoints(
    start: TileCoord,
) -> Result<Vec<TileCoord>, SourceQualificationError> {
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
