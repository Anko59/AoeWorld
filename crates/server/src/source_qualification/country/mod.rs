//! Separate bounded country diagnostics; never relax the 50k/1:1 qualification case.
use super::{ORDINARY_ACTIVATION_SEARCH_CHUNKS, SourceQualificationError};
use crate::{PageResidency, load_map_packages};
use aoe_core::TileCoord;
use aoe_map::{
    DetailProfile, EnvironmentPageProvider, LayerProvenance, MapPackage, MovementOutcome,
};
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
    pub live_activation: bool,
    pub hardware_qualified: bool,
}

#[derive(Debug, Serialize)]
pub struct CountryRouteProbe {
    pub destination: [i32; 2],
    pub outcome: &'static str,
    pub path_tiles: usize,
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
    let outcome = world.terrain().search_start_for_recipe(
        world.config(),
        package.generation_recipe_version,
        ORDINARY_ACTIVATION_SEARCH_CHUNKS,
        || false,
    )?;
    let (ordinary_start, start) = start_result(outcome);
    let mut routes = Vec::new();
    let mut start_neighbourhood = Vec::new();
    let mut local_component = None;
    let mut native_local_orders = Vec::new();
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
            let (outcome, path_tiles) = match world
                .terrain()
                .route_outcome(TileCoord::new(x, y), destination)
            {
                Some(MovementOutcome::Path(path)) => ("path", path.tiles.len()),
                Some(MovementOutcome::InvalidDestination) => ("invalid_destination", 0),
                Some(MovementOutcome::Unreachable) => ("unreachable", 0),
                Some(MovementOutcome::BudgetExceeded) => ("budget_exceeded", 0),
                None => ("no_map_planner", 0),
            };
            routes.push(CountryRouteProbe {
                destination: [destination.x, destination.y],
                outcome,
                path_tiles,
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
        live_activation: false,
        hardware_qualified: false,
    })
}

fn supported_country(package: &MapPackage) -> bool {
    package.request.detail_profile == DetailProfile::LandscapeV2
        && package.generation_recipe_version == 9
        && package.request.year_ce == 600
        && package.request.compression.numerator == 30
        && package.request.compression.denominator == 1
        && package.estimate.tiles_per_side == 20_000
        && package.estimate.effective_side_meters == 1_200_000
        && !package.source_locks.is_empty()
        && package.provenance.elevation == LayerProvenance::SourceDerived
        && package.environment.samples_per_axis == 1024
        && package.environment.water_samples_per_axis() == Some(128)
        && package.environment.vegetation_samples_per_axis() == Some(128)
        && package.environment.historical_samples_per_axis() == Some(1024)
}

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
