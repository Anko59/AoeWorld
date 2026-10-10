//! Separate bounded country diagnostics; never relax the 50k/1:1 qualification case.
use super::{ORDINARY_ACTIVATION_SEARCH_CHUNKS, SourceQualificationError};
use crate::{PageResidency, load_map_packages};
use aoe_core::TileCoord;
use aoe_map::{EnvironmentPageProvider, LayerProvenance, MapPackage, MovementOutcome};
use aoe_simulation::{GameWorld, StartSearchResult};
use serde::Serialize;
use std::{path::Path, sync::Arc};

#[derive(Debug, Serialize)]
pub struct SourceCountryProbe {
    pub policy: &'static str,
    pub content_hash: String,
    pub source_lock_count: usize,
    pub indexed_pages: usize,
    pub tiles_per_side: u64,
    pub typed_hydrology: bool,
    pub ordinary_start: &'static str,
    pub start: Option<[i32; 2]>,
    pub routes: Vec<CountryRouteProbe>,
    pub live_activation: bool,
    pub hardware_qualified: bool,
}

#[derive(Debug, Serialize)]
pub struct CountryRouteProbe {
    pub destination: [i32; 2],
    pub outcome: &'static str,
    pub path_tiles: usize,
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
    if let Some([x, y]) = start {
        for (dx, dy) in [(128, 0), (0, 128), (-128, 0), (0, -128)] {
            let destination = TileCoord::new(x + dx, y + dy);
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
            });
        }
    }
    Ok(SourceCountryProbe {
        policy: "landscape-country-ordinary-start-four-local-orders-v1",
        content_hash: content_hash.to_owned(),
        source_lock_count: package.source_locks.len(),
        indexed_pages: provider.indexed_pages(),
        tiles_per_side: package.estimate.tiles_per_side,
        typed_hydrology: package.environment.hydrology_evidence.is_some(),
        ordinary_start,
        start,
        routes,
        live_activation: false,
        hardware_qualified: false,
    })
}

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
