use crate::MapStoreError;
use aoe_simulation::GameWorldError;
use serde::Serialize;

const MAX_ROUTE_TICKS: u64 = 1_200_000;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SourceQualificationProgress {
    pub tick: u64,
    pub leg: usize,
    pub moved_meters: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LogicalMemoryEvidence {
    pub indexed_page_count: usize,
    pub page_churn_count: usize,
    pub peak_resident_pages_per_provider: usize,
    pub peak_route_navigation_cache_entries: usize,
    pub peak_replay_navigation_cache_entries: usize,
    pub peak_combined_navigation_cache_entries: usize,
    pub peak_route_navigation_cache_logical_retained_bytes: usize,
    pub peak_replay_navigation_cache_logical_retained_bytes: usize,
    /// Logical navigation-cache payload bytes only. Allocator and map
    /// container overhead are excluded; this is not process RSS.
    pub peak_combined_navigation_cache_logical_retained_bytes: usize,
    pub navigation_cache_byte_scope: &'static str,
    pub resource_overlay_change_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProcessRssBytes {
    /// `null` means `/proc/self/status` did not provide a valid `VmRSS`.
    pub start: Option<u64>,
    /// Maximum sampled `VmRSS`, not the kernel's unsampled lifetime high-water mark.
    pub peak: Option<u64>,
    pub end: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResourceLifecycleEvidence {
    pub initial_snapshot_count: u8,
    pub resource_mutation_count: u8,
    pub delta_snapshot_count: u8,
    pub resume_reconnect_snapshot_count: u8,
    pub restart_count: u8,
    pub persisted_snapshot_replay_count: u8,
    pub post_restart_client_snapshot_count: u8,
    pub page_residency_recreation_count: u8,
    pub stored_map_recreation_count: u8,
    pub network_connection_count: u8,
    pub resume_token_reconnect_verified: bool,
    pub changed_world_id_verified: bool,
    pub persisted_snapshot_verified: bool,
    pub post_restart_client_snapshot_verified: bool,
    pub network_resume_snapshot_verified: bool,
    pub network_post_restart_snapshot_verified: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SimulationWork {
    pub movement_ticks: u64,
    pub simulated_seconds: f64,
    pub route_movement_leg_count: u64,
    pub replay_movement_leg_count: u64,
    pub route_repetition_count: u64,
    pub route_moved_meters: f64,
    pub replay_moved_meters: f64,
    pub route_checkpoint_count: usize,
    pub movement_replay_comparison_count: usize,
    pub resource_lifecycle: ResourceLifecycleEvidence,
}

#[derive(Clone, Debug, Serialize)]
pub struct RouteEvidence {
    pub contract: &'static str,
    pub spatial_scope: &'static str,
    pub start_tile: [i32; 2],
    pub alternate_tile: [i32; 2],
    pub offset_tiles: [i32; 2],
    pub spatial_extent_tiles: [i32; 2],
    pub leg_length_tiles: u32,
    pub leg_length_meters: f64,
    pub repetition_count: u64,
    pub required_distance_meters: f64,
    pub movement_ticks: u64,
    pub moved_meters: f64,
    pub replay_hash: String,
    pub replay_matches: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadEvidenceClass {
    SourceBackedQualification,
    BoundedTestContract,
    SparseMaximumContract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadDatasetEvidence {
    VerifiedSourcePackage,
    UnavailableNotClaimed,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SourceWorkloadContract {
    pub tiles_per_side: u64,
    pub physical_side_meters: u64,
    pub evidence_class: WorkloadEvidenceClass,
    pub dataset_evidence: WorkloadDatasetEvidence,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceQualificationReport {
    pub qualification_case: &'static str,
    pub package_hash: String,
    pub schema_version: u16,
    pub generator_version: u16,
    pub generation_recipe_version: u16,
    pub tiles_per_side: u64,
    pub physical_side_meters: u64,
    pub sample_axis: u16,
    pub source_lock_count: usize,
    pub source_lock_ids: Vec<String>,
    pub start_tile: [i32; 2],
    pub activation_component_policy: &'static str,
    pub activation_component_diagnostic: String,
    pub route_waypoints: Vec<[i32; 2]>,
    pub route_evidence: RouteEvidence,
    pub movement_ticks: u64,
    pub simulated_seconds: f64,
    pub moved_meters: f64,
    pub wall_seconds: f64,
    pub replay_hash: String,
    pub replay_matches: bool,
    pub route_checkpoint_count: usize,
    pub indexed_page_count: usize,
    pub page_churn_unique_count: usize,
    pub peak_resident_page_count_per_provider: usize,
    pub evicted_page_reloaded_same_hash: bool,
    pub resource_id: u64,
    pub resource_tile: [i32; 2],
    pub resource_overlay_revision: u64,
    pub resource_overlay_reloaded_equal: bool,
    pub logical_memory: LogicalMemoryEvidence,
    pub process_rss_bytes: ProcessRssBytes,
    pub simulation_work: SimulationWork,
    pub source_workload_contracts: Vec<SourceWorkloadContract>,
}

pub(super) fn ensure_report_agreement(
    report: &SourceQualificationReport,
    tick_hz: u32,
) -> Result<(), SourceQualificationError> {
    ensure_equal(
        "route_evidence.movement_ticks",
        report.route_evidence.movement_ticks,
        report.movement_ticks,
    )?;
    ensure_equal(
        "simulation_work.movement_ticks",
        report.simulation_work.movement_ticks,
        report.movement_ticks,
    )?;
    ensure_equal(
        "route_evidence.repetition_count",
        report.route_evidence.repetition_count,
        super::route::REQUIRED_REPETITIONS,
    )?;
    ensure_equal(
        "simulation_work.route_repetition_count",
        report.simulation_work.route_repetition_count,
        super::route::REQUIRED_REPETITIONS,
    )?;
    ensure_equal(
        "simulation_work.route_movement_leg_count",
        report.simulation_work.route_movement_leg_count,
        super::route::REQUIRED_REPETITIONS,
    )?;
    ensure_equal(
        "simulation_work.replay_movement_leg_count",
        report.simulation_work.replay_movement_leg_count,
        super::route::REQUIRED_REPETITIONS,
    )?;
    ensure_exact_distance(
        "route_evidence.moved_meters",
        report.route_evidence.moved_meters,
    )?;
    ensure_exact_distance("moved_meters", report.moved_meters)?;
    ensure_exact_distance(
        "simulation_work.route_moved_meters",
        report.simulation_work.route_moved_meters,
    )?;
    ensure_exact_distance(
        "simulation_work.replay_moved_meters",
        report.simulation_work.replay_moved_meters,
    )?;
    ensure_equal(
        "route_evidence.replay_hash",
        report.route_evidence.replay_hash.as_str(),
        report.replay_hash.as_str(),
    )?;
    if report.route_evidence.replay_hash.len() != 64 {
        return Err(SourceQualificationError::ReportFieldMismatch {
            field: "route_evidence.replay_hash.length",
            left: report.route_evidence.replay_hash.len().to_string(),
            right: "64".to_owned(),
        });
    }
    for (field, replay_matches) in [
        (
            "route_evidence.replay_matches",
            report.route_evidence.replay_matches,
        ),
        ("replay_matches", report.replay_matches),
    ] {
        if !replay_matches {
            return Err(SourceQualificationError::ReportFieldMismatch {
                field,
                left: "false".to_owned(),
                right: "true".to_owned(),
            });
        }
    }
    ensure_equal(
        "route_checkpoint_count",
        report.route_checkpoint_count,
        report.simulation_work.route_checkpoint_count,
    )?;
    ensure_equal(
        "start_tile",
        report.start_tile,
        report.route_evidence.start_tile,
    )?;
    ensure_equal(
        "route_waypoints.start",
        report.route_waypoints.first().copied(),
        Some(report.route_evidence.start_tile),
    )?;
    ensure_equal(
        "route_waypoints.alternate",
        report.route_waypoints.get(1).copied(),
        Some(report.route_evidence.alternate_tile),
    )?;
    ensure_equal("route_waypoints.count", report.route_waypoints.len(), 2)?;
    if report.route_evidence.spatial_extent_tiles != [2, 0]
        || report.route_evidence.spatial_scope != "ordinary_activation_component_local"
    {
        return Err(SourceQualificationError::ReportFieldMismatch {
            field: "route_evidence.spatial_extent_tiles",
            left: format!(
                "{:?}/{}",
                report.route_evidence.spatial_extent_tiles, report.route_evidence.spatial_scope
            ),
            right: "[2, 0]/ordinary_activation_component_local".to_owned(),
        });
    }
    let expected_seconds = report.movement_ticks as f64 / f64::from(tick_hz);
    ensure_exact_float(
        "simulated_seconds",
        report.simulated_seconds,
        expected_seconds,
    )?;
    ensure_exact_float(
        "simulation_work.simulated_seconds",
        report.simulation_work.simulated_seconds,
        expected_seconds,
    )
}

fn ensure_equal<T: PartialEq + std::fmt::Debug>(
    field: &'static str,
    left: T,
    right: T,
) -> Result<(), SourceQualificationError> {
    if left != right {
        return Err(SourceQualificationError::ReportFieldMismatch {
            field,
            left: format!("{left:?}"),
            right: format!("{right:?}"),
        });
    }
    Ok(())
}

fn ensure_exact_distance(
    field: &'static str,
    observed: f64,
) -> Result<(), SourceQualificationError> {
    ensure_exact_float(field, observed, super::route::REQUIRED_TRAVEL_METERS)
}

fn ensure_exact_float(
    field: &'static str,
    left: f64,
    right: f64,
) -> Result<(), SourceQualificationError> {
    if left != right {
        return Err(SourceQualificationError::ReportFieldMismatch {
            field,
            left: format!("{left:.12}"),
            right: format!("{right:.12}"),
        });
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum SourceQualificationError {
    #[error(transparent)]
    Store(#[from] MapStoreError),
    #[error(
        "package `{requested}` is absent from package directory; verified packages: {available:?}"
    )]
    UnknownPackage {
        requested: String,
        available: Vec<String>,
    },
    #[error(
        "source qualification supports only a source-backed 50,000-tile square at 1:1 compression (100 km physical side)"
    )]
    UnsupportedPackage,
    #[error("source qualification requires generation recipe 5, found recipe {0}")]
    UnsupportedGenerationRecipe(u16),
    #[error(
        "source package indexes {server_pages} server pages but the qualification walker enumerates {walked_pages}"
    )]
    PageIndexMismatch {
        server_pages: usize,
        walked_pages: usize,
    },
    #[error(
        "source package has only {indexed_pages} verified pages; eviction qualification requires at least {required_unique_pages}"
    )]
    InsufficientPageChurn {
        indexed_pages: usize,
        required_unique_pages: usize,
    },
    #[error(
        "page residency reached {observed_pages} pages, exceeding its fixed {maximum_pages}-page bound"
    )]
    ResidentPageLimitExceeded {
        observed_pages: usize,
        maximum_pages: usize,
    },
    #[error("source package has no verified environment pages")]
    NoEnvironmentPages,
    #[error("fixed 64 by 64 resource scan found no resource node")]
    NoResource,
    #[error("source map has no playable starting area within the standard bounded search")]
    NoStart,
    #[error("fixed route waypoint ({x},{y}) is impassable")]
    ImpassableWaypoint { x: i32, y: i32 },
    #[error("fixed route waypoint ({x},{y}) is unreachable: {diagnostic}")]
    UnreachableWaypoint { x: i32, y: i32, diagnostic: String },
    #[error(
        "bounded activation component diagnostic exceeded {phase} at {observed}/{maximum}: {diagnostic}"
    )]
    ActivationLimit {
        phase: &'static str,
        observed: u64,
        maximum: u64,
        diagnostic: String,
    },
    #[error(
        "fixed repeated route has no crossable cardinal leg from start {start:?}: {diagnostic}"
    )]
    FixedRouteLegUnavailable { start: [i32; 2], diagnostic: String },
    #[error(
        "fixed repeated route requires at least {required} ticks but the configured bound is {maximum}"
    )]
    FixedRouteTickBound { required: u64, maximum: u64 },
    #[error(
        "continuous fixed route expected exactly {expected} movement ticks but observed {observed}"
    )]
    MovementTickMismatch { expected: u64, observed: u64 },
    #[error("source route expected exactly {expected} repetitions but observed {observed}")]
    MovementRepetitionMismatch { expected: u64, observed: u64 },
    #[error("source route expected exactly {expected:.3} m but observed {observed:.3} m")]
    MovementDistanceMismatch { expected: f64, observed: f64 },
    #[error("source route expected endpoint {expected:?} but observed {observed:?}")]
    MovementEndpointMismatch {
        expected: [i32; 2],
        observed: [i32; 2],
    },
    #[error("source route expected {expected} queued waypoints but observed {observed}")]
    MovementQueueMismatch { expected: usize, observed: usize },
    #[error(
        "replay distance {replay_meters:.3} m does not match route distance {route_meters:.3} m"
    )]
    ReplayDistanceMismatch {
        route_meters: f64,
        replay_meters: f64,
    },
    #[error("report field `{field}` disagrees: {left} != {right}")]
    ReportFieldMismatch {
        field: &'static str,
        left: String,
        right: String,
    },
    #[error("normal activation start search returned {outcome}; center diagnosis: {diagnostic}")]
    StartSearchLimit {
        outcome: &'static str,
        diagnostic: String,
    },
    #[error("movement command failed: {0}")]
    Movement(#[from] GameWorldError),
    #[error("coordinate conversion failed: {0}")]
    Coordinate(#[from] aoe_core::CoordinateError),
    #[error("resource lifecycle failed: {0}")]
    Resource(#[from] crate::ResourceLifecycleError),
    #[error("map generator rejected the verified provider: {0}")]
    Package(#[from] aoe_map::MapPackageError),
    #[error("source qualification exceeded its {MAX_ROUTE_TICKS}-tick hard bound")]
    TickLimit,
    #[error("replay diverged at tick {0}")]
    ReplayDiverged(u64),
    #[error("persisted resource overlay changed across reload")]
    OverlayMismatch,
    #[error("resource lifecycle evidence mismatch at {stage}")]
    Lifecycle { stage: &'static str },
    #[error("page residency failed: {0}")]
    Page(#[from] aoe_map::EnvironmentPageError),
    #[error("page payload hash could not be computed: {0}")]
    Environment(#[from] aoe_map::EnvironmentError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("qualification package JSON could not be encoded: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests;
