//! Separate bounded country diagnostics; never relax the 50k/1:1 qualification case.
use super::{ORDINARY_ACTIVATION_SEARCH_CHUNKS, SourceQualificationError};
use crate::{PageResidency, load_map_packages};
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageProvider, LayerProvenance, MapPackage, RoutePlannerPoll};
use aoe_simulation::{GameWorld, StartSearchResult};
use serde::Serialize;
use std::{path::Path, sync::Arc};

mod component;
mod diagnostics;
mod movement;
use diagnostics::{DIRECTIONS, Diagnostics, LOCAL_DISTANCE, StraightLineProbe, TileObservation};

#[derive(Debug, Serialize)]
pub struct SourceCountryProbe {
    pub policy: &'static str,
    pub content_hash: String,
    pub source_lock_count: usize,
    pub indexed_pages: usize,
    pub tiles_per_side: u64,
    pub typed_hydrology: bool,
    pub hydrology_index: Option<aoe_map::HydrologyEvidenceIndex>,
    pub ordinary_start: &'static str,
    pub start: Option<[i32; 2]>,
    pub start_neighbourhood: Vec<TileObservation>,
    pub local_component: Option<component::ComponentObservation>,
    pub native_local_orders: Vec<movement::NativeLocalOrder>,
    pub routes: Vec<CountryRouteProbe>,
    /// The planner alone on the 1,024-tile destinations of the far orders.
    pub far_routes: Vec<PlannedRoute>,
    pub live_activation: bool,
    pub hardware_qualified: bool,
}

/// One order planned by the planner native movement uses, leg by leg.
#[derive(Clone, Debug, Serialize)]
pub struct PlannedRoute {
    pub destination: [i32; 2],
    pub outcome: &'static str,
    /// Tiles of the proven route; for `budget_exceeded` or `unreachable`, of
    /// the partial route a unit walks before its planner gives up.
    pub path_tiles: usize,
    /// Deterministic work spent before the first segment was emitted; the
    /// simulation spends at most 16,384 per tick.
    pub first_segment_work: Option<u32>,
    pub total_work: u32,
    /// Peak logical entries retained (frontier, records, route).
    pub peak_retained_entries: usize,
}

#[derive(Debug, Serialize)]
pub struct CountryRouteProbe {
    pub destination: [i32; 2],
    pub outcome: &'static str,
    pub path_tiles: usize,
    pub planner: PlannedRoute,
    pub endpoint_neighbourhood: Vec<TileObservation>,
    pub straight_line: StraightLineProbe,
}

/// Checks ordinary start work and four fixed 256m local orders on the real
/// provider. This is not long-distance movement, live activation or hardware QA.
pub fn run_source_country_probe(
    directory: &Path,
    content_hash: &str,
) -> Result<SourceCountryProbe, SourceQualificationError> {
    let packages = load_map_packages(Some(directory))?;
    let package = packages.get(content_hash).cloned().ok_or_else(|| {
        SourceQualificationError::UnknownPackage {
            requested: content_hash.to_owned(),
            available: packages.keys().cloned().collect(),
        }
    })?;
    package
        .validate()
        .map_err(|_| SourceQualificationError::UnsupportedPackage)?;
    if !supported_country(&package) {
        return Err(SourceQualificationError::UnsupportedPackage);
    }
    let provider = PageResidency::open(directory, &package, &|| false)?;
    let world = GameWorld::from_page_provider(
        package.clone(),
        provider.clone() as Arc<dyn EnvironmentPageProvider>,
    )?;
    let outcome = world.terrain().search_start_checked(
        world.config(),
        ORDINARY_ACTIVATION_SEARCH_CHUNKS,
        || false,
    )?;
    let (ordinary_start, start) = start_result(outcome);
    let mut routes = Vec::new();
    let mut start_neighbourhood = Vec::new();
    let mut local_component = None;
    let mut native_local_orders = Vec::new();
    let mut far_routes = Vec::new();
    if let Some([x, y]) = start {
        let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() else {
            return Err(SourceQualificationError::UnsupportedPackage);
        };
        let diagnostics = Diagnostics {
            terrain: world.terrain(),
            generator,
            config: world.config(),
            package: &package,
            provider: &provider,
        };
        // Preserve planner calls and ordering before collecting extra observations.
        for (dx, dy) in DIRECTIONS {
            let destination = TileCoord::new(x + dx * LOCAL_DISTANCE, y + dy * LOCAL_DISTANCE);
            let planner = planned_route(world.terrain(), TileCoord::new(x, y), destination)?;
            let (outcome, path_tiles) = (planner.outcome, planner.path_tiles);
            routes.push(CountryRouteProbe {
                destination: [destination.x, destination.y],
                outcome,
                path_tiles,
                planner,
                endpoint_neighbourhood: diagnostics.neighbourhood(destination)?,
                straight_line: diagnostics.straight_line(TileCoord::new(x, y), (dx, dy))?,
            });
        }
        start_neighbourhood = diagnostics.neighbourhood(TileCoord::new(x, y))?;
        local_component = Some(component::observe(
            world.terrain(),
            world.config(),
            TileCoord::new(x, y),
        )?);
        for direction in DIRECTIONS {
            let far = movement::OrderCase::Far1024.destination(TileCoord::new(x, y), direction);
            far_routes.push(planned_route(world.terrain(), TileCoord::new(x, y), far)?);
        }
        native_local_orders = movement::observe(&package, &provider, TileCoord::new(x, y))?;
    }
    Ok(SourceCountryProbe {
        policy: "landscape-country-ordinary-start-four-local-orders-v2-observations",
        content_hash: content_hash.to_owned(),
        source_lock_count: package.source_locks.len(),
        indexed_pages: provider.indexed_pages(),
        tiles_per_side: package.estimate.tiles_per_side,
        typed_hydrology: package.environment.hydrology_evidence.is_some(),
        hydrology_index: package.environment.hydrology_evidence.clone(),
        ordinary_start,
        start,
        start_neighbourhood,
        local_component,
        native_local_orders,
        routes,
        far_routes,
        live_activation: false,
        hardware_qualified: false,
    })
}

/// Plans a route with the planner native orders use, leg by leg, so this
/// result agrees with what a unit ordered to the same destination achieves.
fn planned_route(
    terrain: &aoe_simulation::Terrain,
    origin: TileCoord,
    destination: TileCoord,
) -> Result<PlannedRoute, SourceQualificationError> {
    let mut route = PlannedRoute {
        destination: [destination.x, destination.y],
        outcome: "no_map_planner",
        path_tiles: 0,
        first_segment_work: None,
        total_work: 0,
        peak_retained_entries: 0,
    };
    let Some(mut planner) =
        terrain.route_planner(origin, destination, aoe_map::MAX_ROUTE_PLANNER_WORK)
    else {
        return Ok(route);
    };
    route.outcome = loop {
        // Every poll emits a segment, ends, or spends one tick of work.
        match terrain.poll_route_planner(&mut planner, PLANNER_TICK_WORK, &|| false) {
            Some(RoutePlannerPoll::Path(path)) => {
                route.first_segment_work.get_or_insert(planner.work());
                route.path_tiles = route.path_tiles.max(1) + path.tiles.len().saturating_sub(1);
            }
            Some(RoutePlannerPoll::Pending) => {}
            Some(RoutePlannerPoll::Complete) => break "path",
            Some(RoutePlannerPoll::InvalidDestination) => break "invalid_destination",
            Some(RoutePlannerPoll::Unreachable) => break "unreachable",
            Some(RoutePlannerPoll::SearchLimit) => break "budget_exceeded",
            Some(RoutePlannerPoll::Environment(error)) => return Err(error.into()),
            None => break "no_map_planner",
        }
    };
    route.total_work = planner.work();
    route.peak_retained_entries = planner.peak_retained_entries();
    Ok(route)
}

/// The simulation's shared route work per tick (`MAX_ROUTE_WORK_PER_TICK`).
const PLANNER_TICK_WORK: u32 = 16_384;

fn supported_country(package: &MapPackage) -> bool {
    package.request.year_ce == 600
        && package.request.compression.numerator == 30
        && package.request.compression.denominator == 1
        && package.estimate.tiles_per_side == 20_000
        && package.estimate.effective_side_meters == 1_200_000
        && !package.source_locks.is_empty()
        && package.provenance.elevation == LayerProvenance::SourceDerived
        && package.environment.samples_per_axis == LANDSCAPE.elevation
        && package.environment.water_samples_per_axis() == Some(LANDSCAPE.water)
        && package.environment.vegetation_samples_per_axis() == Some(LANDSCAPE.vegetation)
        && package.environment.historical_samples_per_axis() == Some(LANDSCAPE.historical)
}

const LANDSCAPE: aoe_map::OverviewFieldAxes = aoe_map::OverviewFieldAxes::LANDSCAPE;

fn start_result(outcome: StartSearchResult) -> (&'static str, Option<[i32; 2]>) {
    match outcome {
        StartSearchResult::Found(tile) => ("found", Some([tile.x, tile.y])),
        StartSearchResult::Unavailable => ("unavailable", None),
        StartSearchResult::LimitReached => ("limit_reached", None),
        StartSearchResult::Cancelled => ("cancelled", None),
    }
}

#[cfg(test)]
mod tests;
