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
    pub route_waypoints: Vec<[i32; 2]>,
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
    #[error(
        "source package indexes {server_pages} server pages but the qualification walker enumerates {walked_pages}"
    )]
    PageIndexMismatch {
        server_pages: usize,
        walked_pages: usize,
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
    #[error("page residency failed: {0}")]
    Page(#[from] aoe_map::EnvironmentPageError),
    #[error("page payload hash could not be computed: {0}")]
    Environment(#[from] aoe_map::EnvironmentError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("qualification package JSON could not be encoded: {0}")]
    Json(#[from] serde_json::Error),
}
