//! Bounded qualification against immutable, verified source-backed packages.

mod diagnostics;
mod report;
mod route;

pub use report::{
    SourceQualificationError, SourceQualificationProgress, SourceQualificationReport,
};

use crate::{GameplayService, PageResidency, load_map_packages};
use aoe_core::{PlayerId, TileCoord, WorldPosition};
use aoe_map::{
    EnvironmentPageKey, EnvironmentPageProvider, FieldPyramid, LayerProvenance, MapPackage,
    PageLayer, ResourceNode,
};
use aoe_simulation::{GameWorld, GameWorldError, StartSearchResult};
use diagnostics::start_diagnostic;
use route::{classify_route_failure, issue_leg};
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

const MAX_AXIS_TILES: u64 = 50_000;
const MAX_ROUTE_TICKS: u64 = 1_200_000;
const MAX_RESOURCE_SCAN_SIDE: i32 = 64;
const HASH_CHECK_INTERVAL: u64 = 8_192;
const MAX_RESIDENT_PAGES: usize = 128;
const QUALIFICATION_CASE: &str = "source-backed-100km-50k-tiles-1-to-1";

/// Exercises overlay durability, verified page eviction, and physical
/// movement on one source-backed package. Movement follows fixed opposite
/// points on the package centerline; no seed, route, or destination search is
/// performed to obtain a successful result.
pub async fn run_source_qualification(
    package_directory: &Path,
    content_hash: &str,
    max_ticks: u64,
    mut progress: impl FnMut(SourceQualificationProgress),
) -> Result<SourceQualificationReport, SourceQualificationError> {
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
    if package.source_locks.is_empty()
        || package.request.compression.numerator != 1
        || package.request.compression.denominator != 1
        || package.estimate.tiles_per_side != MAX_AXIS_TILES
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
        return Err(SourceQualificationError::NoEnvironmentPages);
    }
    if keys.len() != indexed_page_count {
        return Err(SourceQualificationError::PageIndexMismatch {
            server_pages: indexed_page_count,
            walked_pages: keys.len(),
        });
    }
    let generator = package.generator_with_page_provider(provider.clone())?;
    let node = find_center_resource(&generator, package.estimate.tiles_per_side as i32)?;
    let activation_probe = GameWorld::from_page_provider(
        package.clone(),
        provider.clone() as Arc<dyn EnvironmentPageProvider>,
    )?;
    let activation_config = activation_probe.config();
    let activation_start = activation_probe.terrain().search_start_for_recipe(
        activation_config,
        package.generation_recipe_version,
        64,
        || false,
    )?;
    let Some(_standard_start) = (match activation_start {
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
            64,
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
    let scratch = QualificationDirectory::new()?;
    let gameplay = GameplayService::from_stored_map(
        package.clone(),
        Some(provider.clone() as Arc<dyn EnvironmentPageProvider>),
        Some(scratch.path.clone()),
        &|| false,
    )?
    .ok_or(SourceQualificationError::NoStart)?;
    let depleted = gameplay
        .deplete_resource_persisted(node.id, node.initial_amount)
        .await?;
    if depleted.depletion.remaining != 0 || !depleted.depletion.became_nonblocking {
        return Err(SourceQualificationError::NoResource);
    }
    let overlay_snapshot = gameplay
        .resource_snapshot()
        .await
        .ok_or(SourceQualificationError::NoResource)?;
    let overlay_revision = overlay_snapshot.revision;

    let first_key = *keys
        .first()
        .ok_or(SourceQualificationError::NoEnvironmentPages)?;
    let first_hash = provider.page(first_key, &|| false)?.content_hash()?;
    let mut peak_resident_pages = provider.resident_pages();
    for key in &keys {
        provider.page(*key, &|| false)?;
        peak_resident_pages = peak_resident_pages.max(provider.resident_pages());
        if peak_resident_pages > MAX_RESIDENT_PAGES {
            return Err(SourceQualificationError::NoEnvironmentPages);
        }
    }
    let reloaded_hash = provider.page(first_key, &|| false)?.content_hash()?;
    let evicted_page_reloaded_same_hash = first_hash == reloaded_hash;
    if !evicted_page_reloaded_same_hash {
        return Err(SourceQualificationError::Page(
            aoe_map::EnvironmentPageError::Corrupt,
        ));
    }

    let restored_provider = PageResidency::open(package_directory, &package, &|| false)?;
    let restored = GameplayService::from_stored_map(
        package.clone(),
        Some(restored_provider as Arc<dyn EnvironmentPageProvider>),
        Some(scratch.path.clone()),
        &|| false,
    )?
    .ok_or(SourceQualificationError::NoStart)?;
    let restored_snapshot = restored
        .resource_snapshot()
        .await
        .ok_or(SourceQualificationError::NoResource)?;
    let resource_overlay_reloaded_equal = overlay_snapshot == restored_snapshot;
    if !resource_overlay_reloaded_equal {
        return Err(SourceQualificationError::OverlayMismatch);
    }

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
    let config = world.config();
    let start = match world.terrain().search_start_for_recipe(
        config,
        package.generation_recipe_version,
        64,
        || false,
    )? {
        StartSearchResult::Found(tile) => tile,
        StartSearchResult::Unavailable
        | StartSearchResult::LimitReached
        | StartSearchResult::Cancelled => return Err(SourceQualificationError::NoStart),
    };
    let unit = world.spawn_unit(PlayerId(0), WorldPosition::from_tile_center(start)?)?;
    let replay_unit = replay.spawn_unit(PlayerId(0), WorldPosition::from_tile_center(start)?)?;
    let axis = config.width_tiles;
    let center_y = (config.height_tiles - 1) / 2;
    let waypoints = [
        TileCoord::new(axis - 2, center_y),
        TileCoord::new(1, center_y),
    ];
    issue_leg(
        &mut world,
        &generator,
        &package,
        &movement_provider,
        unit,
        waypoints[0],
    )?;
    issue_leg(
        &mut replay,
        &generator,
        &package,
        &replay_provider,
        replay_unit,
        waypoints[0],
    )?;

    let mut leg = 0_usize;
    let mut replay_leg = 0_usize;
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
    let mut checkpoints = 0_usize;
    let mut movement_ticks = 0_u64;
    let mut peak_resident_pages = peak_resident_pages
        .max(provider.resident_pages())
        .max(movement_provider.resident_pages())
        .max(replay_provider.resident_pages());
    for tick in 1..=max_ticks {
        world.advance();
        replay.advance();
        movement_ticks = tick;
        let state = world.unit(unit).ok_or(GameWorldError::UnknownEntity)?;
        let replay_state = replay
            .unit(replay_unit)
            .ok_or(GameWorldError::UnknownEntity)?;
        moved_subunits += displacement(last_position, state.position);
        replay_moved_subunits += displacement(replay_last_position, replay_state.position);
        last_position = state.position;
        replay_last_position = replay_state.position;
        peak_resident_pages = peak_resident_pages
            .max(provider.resident_pages())
            .max(movement_provider.resident_pages())
            .max(replay_provider.resident_pages());
        if let Some(error) = world.movement_failure(unit) {
            return Err(classify_route_failure(
                error,
                waypoints[leg.min(waypoints.len() - 1)],
            ));
        }
        if let Some(error) = replay.movement_failure(replay_unit) {
            return Err(classify_route_failure(
                error,
                waypoints[replay_leg.min(waypoints.len() - 1)],
            ));
        }
        if tick % HASH_CHECK_INTERVAL == 0 {
            let route_hash = world.canonical_hash();
            let replay_hash = replay.canonical_hash();
            if route_hash != replay_hash || state != replay_state {
                return Err(SourceQualificationError::ReplayDiverged(tick));
            }
            checkpoints += 1;
        }
        if world.movement_order(unit).is_none() {
            leg += 1;
            if leg < waypoints.len() {
                issue_leg(
                    &mut world,
                    &generator,
                    &package,
                    &movement_provider,
                    unit,
                    waypoints[leg],
                )?;
            }
        }
        if replay.movement_order(replay_unit).is_none() {
            replay_leg += 1;
            if replay_leg < waypoints.len() {
                issue_leg(
                    &mut replay,
                    &generator,
                    &package,
                    &replay_provider,
                    replay_unit,
                    waypoints[replay_leg],
                )?;
            }
        }
        if leg == waypoints.len() && replay_leg == waypoints.len() {
            let route_hash = world.canonical_hash();
            let replay_hash = replay.canonical_hash();
            if route_hash != replay_hash || state != replay_state {
                return Err(SourceQualificationError::ReplayDiverged(tick));
            }
            break;
        }
        if tick % 50_000 == 0 {
            progress(SourceQualificationProgress {
                tick,
                leg,
                moved_meters: moved_subunits / 1_024.0 * 2.0,
            });
        }
        if tick == max_ticks {
            return Err(SourceQualificationError::TickLimit);
        }
    }
    let route_hash = world.canonical_hash();
    let replay_hash = replay.canonical_hash();
    if route_hash != replay_hash {
        return Err(SourceQualificationError::ReplayDiverged(movement_ticks));
    }
    let moved_meters = moved_subunits / 1_024.0 * 2.0;
    if moved_meters < 100_000.0 || (replay_moved_subunits / 1_024.0 * 2.0) != moved_meters {
        return Err(SourceQualificationError::InsufficientDistance(moved_meters));
    }
    let source_lock_ids = package
        .source_locks
        .iter()
        .map(|source| source.id.clone())
        .collect::<Vec<_>>();
    Ok(SourceQualificationReport {
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
        start_tile: [start.x, start.y],
        route_waypoints: waypoints.iter().map(|tile| [tile.x, tile.y]).collect(),
        movement_ticks,
        simulated_seconds: movement_ticks as f64 / f64::from(config.tick_hz),
        moved_meters,
        wall_seconds: wall_started.elapsed().as_secs_f64(),
        replay_hash: hex(&route_hash),
        replay_matches: true,
        route_checkpoint_count: checkpoints,
        indexed_page_count: provider.indexed_pages(),
        page_churn_unique_count: keys.len(),
        peak_resident_page_count_per_provider: peak_resident_pages,
        evicted_page_reloaded_same_hash,
        resource_id: node.id,
        resource_tile: [node.tile.x, node.tile.y],
        resource_overlay_revision: overlay_revision,
        resource_overlay_reloaded_equal,
    })
}

fn displacement(from: WorldPosition, to: WorldPosition) -> f64 {
    let dx = f64::from(to.x) - f64::from(from.x);
    let dy = f64::from(to.y) - f64::from(from.y);
    dx.hypot(dy)
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

fn page_keys(package: &MapPackage) -> Vec<EnvironmentPageKey> {
    let mut keys = Vec::new();
    append_keys(
        &mut keys,
        PageLayer::Elevation,
        &package.environment.elevation,
    );
    if let Some(field) = &package.environment.water {
        append_keys(&mut keys, PageLayer::Water, field);
    }
    if let Some(field) = &package.environment.vegetation {
        append_keys(&mut keys, PageLayer::Vegetation, field);
    }
    if let Some(field) = &package.environment.historical_land_use {
        append_keys(&mut keys, PageLayer::HistoricalLandUse, field);
    }
    keys
}

fn append_keys(keys: &mut Vec<EnvironmentPageKey>, layer: PageLayer, field: &FieldPyramid) {
    for (level, metadata) in field.levels.iter().enumerate() {
        let count = metadata.samples_per_axis.div_ceil(64);
        for y in 0..count {
            for x in 0..count {
                keys.push(EnvironmentPageKey {
                    layer,
                    level: level as u8,
                    x,
                    y,
                });
            }
        }
    }
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
