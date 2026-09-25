//! Bounded qualification against immutable, verified source-backed packages.

mod activation;
mod diagnostics;
mod lifecycle;
mod metrics;
mod movement;
mod network;
mod pages;
mod report;
mod route;
mod workload;

use report::{LogicalMemoryEvidence, SimulationWork, ensure_report_agreement};
pub use report::{
    SourceQualificationError, SourceQualificationProgress, SourceQualificationReport,
};

use crate::{GameplayService, PageResidency, load_map_packages};
use aoe_core::TileCoord;
#[cfg(test)]
use aoe_map::MapPackage;
use aoe_map::{EnvironmentPageProvider, LayerProvenance, ResourceNode};
use aoe_simulation::{GameWorld, StartSearchResult};
use diagnostics::start_diagnostic;
use movement::{MovementInput, exercise_movement};
use pages::page_keys;
#[cfg(test)]
use pages::pyramid_page_count;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

#[cfg(test)]
mod tests;

const QUALIFICATION_AXIS_TILES: u64 = 50_000;
const MAX_ROUTE_TICKS: u64 = 1_200_000;
const ORDINARY_ACTIVATION_SEARCH_CHUNKS: usize = 64;
const MAX_RESOURCE_SCAN_SIDE: i32 = 64;
const MAX_RESIDENT_PAGES: usize = 128;
const QUALIFICATION_CASE: &str = "source-backed-100km-50k-tiles-1-to-1";
const NAVIGATION_CACHE_BYTE_SCOPE: &str =
    "logical payload only; excludes allocator and map container overhead";

/// Exercises overlay durability, verified page eviction, and physical
/// movement on one source-backed package. Movement follows deterministic
/// fixed cardinal repetitions inside the ordinary activation component;
/// travel distance is accumulated without claiming cross-map displacement.
pub async fn run_source_qualification(
    package_directory: &Path,
    content_hash: &str,
    max_ticks: u64,
    mut progress: impl FnMut(SourceQualificationProgress),
) -> Result<SourceQualificationReport, SourceQualificationError> {
    let mut rss = metrics::ProcessRssSampler::start();
    if max_ticks == 0 || max_ticks > MAX_ROUTE_TICKS {
        return Err(SourceQualificationError::TickLimit);
    }
    let packages = load_map_packages(Some(package_directory))?;
    let package = packages.get(content_hash).cloned().ok_or_else(|| {
        SourceQualificationError::UnknownPackage {
            requested: content_hash.to_owned(),
            available: packages.keys().cloned().collect(),
        }
    })?;
    package
        .validate()
        .map_err(|_| SourceQualificationError::UnsupportedPackage)?;
    if package.generation_recipe_version != aoe_map::GENERATION_RECIPE_VERSION {
        return Err(SourceQualificationError::UnsupportedGenerationRecipe(
            package.generation_recipe_version,
        ));
    }
    if package.source_locks.is_empty()
        || package.request.compression.numerator != 1
        || package.request.compression.denominator != 1
        || package.estimate.tiles_per_side != QUALIFICATION_AXIS_TILES
        || package.estimate.game_side_meters != 100_000
        || package.environment.samples_per_axis == 0
        || package.provenance.elevation != LayerProvenance::SourceDerived
    {
        return Err(SourceQualificationError::UnsupportedPackage);
    }
    let keys = page_keys(&package);

    let wall_started = Instant::now();
    let provider = PageResidency::open(package_directory, &package, &|| false)?;
    let indexed_page_count = provider.indexed_pages();
    if indexed_page_count <= MAX_RESIDENT_PAGES {
        return Err(SourceQualificationError::InsufficientPageChurn {
            indexed_pages: indexed_page_count,
            required_unique_pages: MAX_RESIDENT_PAGES + 1,
        });
    }
    if keys.len() != indexed_page_count {
        return Err(SourceQualificationError::PageIndexMismatch {
            server_pages: indexed_page_count,
            walked_pages: keys.len(),
        });
    }
    let generator = package.generator_with_page_provider(provider.clone())?;
    let node = find_center_resource(&generator, package.estimate.tiles_per_side as i32)?;
    rss.observe();
    let activation_probe = GameWorld::from_page_provider(
        package.clone(),
        provider.clone() as Arc<dyn EnvironmentPageProvider>,
    )?;
    let activation_config = activation_probe.config();
    let activation_start = activation_probe.terrain().search_start_for_recipe(
        activation_config,
        package.generation_recipe_version,
        ORDINARY_ACTIVATION_SEARCH_CHUNKS,
        || false,
    )?;
    let Some(standard_start) = (match activation_start {
        StartSearchResult::Found(tile) => Some(tile),
        StartSearchResult::Unavailable => return Err(SourceQualificationError::NoStart),
        StartSearchResult::LimitReached | StartSearchResult::Cancelled => None,
    }) else {
        let diagnostic = start_diagnostic(
            activation_probe.terrain(),
            &generator,
            &package,
            provider.as_ref(),
            activation_config,
            ORDINARY_ACTIVATION_SEARCH_CHUNKS,
            None,
        )?;
        let outcome = match activation_start {
            StartSearchResult::LimitReached => "limit_reached",
            StartSearchResult::Cancelled => "cancelled",
            StartSearchResult::Found(_) => "found",
            StartSearchResult::Unavailable => "unavailable",
        };
        return Err(SourceQualificationError::StartSearchLimit {
            outcome,
            diagnostic,
        });
    };
    let activation_component_diagnostic = activation::ordinary_activation_component_diagnostic(
        activation_probe.terrain(),
        &generator,
        standard_start,
        activation_config,
    )?;
    let route = route::plan_fixed_repeated_route(
        activation_probe.terrain(),
        activation_config,
        standard_start,
    )?;
    route.ensure_tick_bound(activation_config, max_ticks)?;
    let scratch = QualificationDirectory::new()?;
    let gameplay = GameplayService::from_stored_map(
        package.clone(),
        Some(provider.clone() as Arc<dyn EnvironmentPageProvider>),
        Some(scratch.path.clone()),
        &|| false,
    )?
    .ok_or(SourceQualificationError::NoStart)?;
    let lifecycle_run = lifecycle::exercise_resource_lifecycle(
        gameplay,
        &package,
        package_directory,
        &scratch.path,
        &node,
    )
    .await?;
    let lifecycle_evidence = lifecycle_run.evidence;
    let overlay_snapshot = lifecycle_run.persisted_snapshot;
    let overlay_revision = overlay_snapshot.revision;
    rss.observe();

    let first_key = *keys
        .first()
        .ok_or(SourceQualificationError::NoEnvironmentPages)?;
    let first_hash = provider.page(first_key, &|| false)?.content_hash()?;
    let mut peak_resident_pages = provider.resident_pages();
    for key in &keys {
        provider.page(*key, &|| false)?;
        peak_resident_pages = peak_resident_pages.max(provider.resident_pages());
        if peak_resident_pages > MAX_RESIDENT_PAGES {
            return Err(SourceQualificationError::ResidentPageLimitExceeded {
                observed_pages: peak_resident_pages,
                maximum_pages: MAX_RESIDENT_PAGES,
            });
        }
    }
    let reloaded_hash = provider.page(first_key, &|| false)?.content_hash()?;
    let evicted_page_reloaded_same_hash = first_hash == reloaded_hash;
    if !evicted_page_reloaded_same_hash {
        return Err(SourceQualificationError::Page(
            aoe_map::EnvironmentPageError::Corrupt,
        ));
    }

    let resource_overlay_reloaded_equal = lifecycle_evidence.persisted_snapshot_verified;

    let movement_provider = PageResidency::open(package_directory, &package, &|| false)?;
    let replay_provider = PageResidency::open(package_directory, &package, &|| false)?;
    let movement_initial_peak = peak_resident_pages
        .max(lifecycle_run.restart_provider_resident_pages)
        .max(provider.resident_pages());
    let movement = exercise_movement(MovementInput {
        package: &package,
        generator: &generator,
        movement_provider,
        replay_provider,
        initial_peak_resident_pages: movement_initial_peak,
        max_ticks,
        route,
        progress: &mut progress,
        rss: &mut rss,
    })?;
    let peak_resident_pages = movement.peak_resident_pages;
    let source_lock_ids = package
        .source_locks
        .iter()
        .map(|source| source.id.clone())
        .collect::<Vec<_>>();
    let process_rss_bytes = rss.finish();
    let replay_hash = hex(&movement.replay_hash);
    let report = SourceQualificationReport {
        qualification_case: QUALIFICATION_CASE,
        package_hash: package.content_hash_hex(),
        schema_version: package.schema_version,
        generator_version: package.generator_version,
        generation_recipe_version: package.generation_recipe_version,
        tiles_per_side: package.estimate.tiles_per_side,
        physical_side_meters: package.estimate.game_side_meters,
        sample_axis: package.environment.samples_per_axis,
        source_lock_count: package.source_locks.len(),
        source_lock_ids,
        start_tile: [movement.start_tile.x, movement.start_tile.y],
        activation_component_policy: activation::ACTIVATION_COMPONENT_POLICY,
        activation_component_diagnostic,
        route_waypoints: movement
            .route
            .waypoints()
            .iter()
            .map(|tile| [tile.x, tile.y])
            .collect(),
        route_evidence: report::RouteEvidence {
            contract: route::ROUTE_CONTRACT,
            spatial_scope: "ordinary_activation_component_local",
            start_tile: [movement.route.start.x, movement.route.start.y],
            alternate_tile: [movement.route.alternate.x, movement.route.alternate.y],
            offset_tiles: [movement.route.offset.0, movement.route.offset.1],
            spatial_extent_tiles: movement.route.spatial_extent_tiles(),
            leg_length_tiles: movement.route.leg_length_tiles,
            leg_length_meters: movement.route.leg_length_meters,
            repetition_count: movement.route.repetitions,
            required_distance_meters: route::REQUIRED_TRAVEL_METERS,
            movement_ticks: movement.movement_ticks,
            moved_meters: movement.route_moved_meters,
            replay_hash: replay_hash.clone(),
            replay_matches: true,
        },
        movement_ticks: movement.movement_ticks,
        simulated_seconds: movement.movement_ticks as f64 / f64::from(activation_config.tick_hz),
        moved_meters: movement.route_moved_meters,
        wall_seconds: wall_started.elapsed().as_secs_f64(),
        replay_hash,
        replay_matches: true,
        route_checkpoint_count: movement.route_checkpoint_count,
        indexed_page_count: provider.indexed_pages(),
        page_churn_unique_count: keys.len(),
        peak_resident_page_count_per_provider: peak_resident_pages,
        evicted_page_reloaded_same_hash,
        resource_id: node.id,
        resource_tile: [node.tile.x, node.tile.y],
        resource_overlay_revision: overlay_revision,
        resource_overlay_reloaded_equal,
        logical_memory: LogicalMemoryEvidence {
            indexed_page_count,
            page_churn_count: keys.len(),
            peak_resident_pages_per_provider: peak_resident_pages,
            peak_route_navigation_cache_entries: movement.navigation_cache_peaks.route_entries,
            peak_replay_navigation_cache_entries: movement.navigation_cache_peaks.replay_entries,
            peak_combined_navigation_cache_entries: movement
                .navigation_cache_peaks
                .combined_entries,
            peak_route_navigation_cache_logical_retained_bytes: movement
                .navigation_cache_peaks
                .route_logical_retained_bytes,
            peak_replay_navigation_cache_logical_retained_bytes: movement
                .navigation_cache_peaks
                .replay_logical_retained_bytes,
            peak_combined_navigation_cache_logical_retained_bytes: movement
                .navigation_cache_peaks
                .combined_logical_retained_bytes,
            navigation_cache_byte_scope: NAVIGATION_CACHE_BYTE_SCOPE,
            resource_overlay_change_count: overlay_snapshot.revision as usize,
        },
        process_rss_bytes,
        simulation_work: SimulationWork {
            movement_ticks: movement.movement_ticks,
            simulated_seconds: movement.movement_ticks as f64
                / f64::from(activation_config.tick_hz),
            route_movement_leg_count: movement.route_movement_leg_count,
            replay_movement_leg_count: movement.replay_movement_leg_count,
            route_repetition_count: movement.route.repetitions,
            route_moved_meters: movement.route_moved_meters,
            replay_moved_meters: movement.replay_moved_meters,
            route_checkpoint_count: movement.route_checkpoint_count,
            movement_replay_comparison_count: movement.movement_replay_comparison_count,
            resource_lifecycle: lifecycle_evidence,
        },
        source_workload_contracts: workload::source_workload_contracts(),
    };
    ensure_report_agreement(&report, activation_config.tick_hz)?;
    Ok(report)
}

fn find_center_resource(
    generator: &aoe_map::MapChunkGenerator,
    axis: i32,
) -> Result<ResourceNode, SourceQualificationError> {
    let center = (axis - 1) / 2;
    let half = MAX_RESOURCE_SCAN_SIDE / 2;
    for y in (center - half).max(0)..(center + half).min(axis) {
        for x in (center - half).max(0)..(center + half).min(axis) {
            if let Some(node) = generator.object_at_with_cancel(TileCoord::new(x, y), &|| false)? {
                return Ok(node);
            }
        }
    }
    Err(SourceQualificationError::NoResource)
}

struct QualificationDirectory {
    path: PathBuf,
}

impl QualificationDirectory {
    fn new() -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        for _ in 0..32 {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "aoeworld-map-source-qualification-{}-{id}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate source qualification state directory",
        ))
    }
}

impl Drop for QualificationDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
