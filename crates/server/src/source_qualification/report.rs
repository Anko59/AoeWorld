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
    #[error("source route moved only {0:.3} m; at least 100,000 m is required")]
    InsufficientDistance(f64),
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
mod tests {
    use super::RouteEvidence;

    #[test]
    fn route_evidence_records_local_extent_repetitions_ticks_distance_and_hash() {
        let evidence = RouteEvidence {
            contract: "fixed-cardinal-repeat-within-ordinary-component-v1",
            spatial_scope: "ordinary_activation_component_local",
            start_tile: [24_999, 24_999],
            alternate_tile: [25_001, 24_999],
            offset_tiles: [2, 0],
            spatial_extent_tiles: [2, 0],
            leg_length_tiles: 2,
            leg_length_meters: 4.0,
            repetition_count: 25_000,
            required_distance_meters: 100_000.0,
            movement_ticks: 675_000,
            moved_meters: 100_000.0,
            replay_hash: "00".repeat(32),
            replay_matches: true,
        };
        let json = serde_json::to_value(evidence).expect("route evidence JSON");

        assert_eq!(json["spatial_extent_tiles"], serde_json::json!([2, 0]));
        assert_eq!(json["leg_length_meters"], 4.0);
        assert_eq!(json["repetition_count"], 25_000);
        assert_eq!(json["movement_ticks"], 675_000);
        assert_eq!(json["moved_meters"], 100_000.0);
        assert_eq!(json["replay_hash"].as_str().expect("hash").len(), 64);
        assert_eq!(json["replay_matches"], true);
    }
}
