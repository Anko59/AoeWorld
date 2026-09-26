use super::{
    report::{
        SourceQualificationError, SourceWorkloadContract, WorkloadDatasetEvidence,
        WorkloadEvidenceClass, ensure_equal, ensure_exact_float,
    },
    route,
};
use serde::Serialize;

const MAX_NAVIGATION_CACHE_LOGICAL_BYTES: usize = 128 * 1024 * 1024;

pub(super) fn ensure_scale_workload_agreement(
    contracts: &[SourceWorkloadContract],
) -> Result<(), SourceQualificationError> {
    let axes = contracts
        .iter()
        .map(|contract| contract.tiles_per_side)
        .collect::<Vec<_>>();
    ensure_equal(
        "source_workload_contracts.axes",
        axes,
        vec![512, 16_384, 50_000, 262_144],
    )?;
    for contract in contracts {
        ensure_equal(
            "source_workload_contracts.physical_side_meters",
            contract.physical_side_meters,
            contract.tiles_per_side * u64::from(aoe_map::GAME_TILE_METERS),
        )?;
        let expected_evidence_class = match (contract.tiles_per_side, &contract.source_evidence) {
            (262_144, _) => WorkloadEvidenceClass::SparseMaximumContract,
            (_, Some(_)) => WorkloadEvidenceClass::SourceBackedQualification,
            (_, None) => WorkloadEvidenceClass::BoundedTestContract,
        };
        ensure_equal(
            "source_workload_contracts.evidence_class",
            contract.evidence_class,
            expected_evidence_class,
        )?;
        ensure_equal(
            "source_workload_contracts.dataset_evidence",
            contract.dataset_evidence,
            if contract.source_evidence.is_some() {
                WorkloadDatasetEvidence::VerifiedSourcePackage
            } else {
                WorkloadDatasetEvidence::UnavailableNotClaimed
            },
        )?;
        if let Some(evidence) = &contract.source_evidence {
            if evidence.package_hash.len() != 64
                || evidence.generation_recipe_version != aoe_map::GENERATION_RECIPE_VERSION
                || evidence.sample_axis == 0
                || evidence.source_lock_count == 0
                || evidence.indexed_page_count == 0
                || evidence.viewport_chunk_sequence.len() != 6
                || evidence.viewport_chunk_sequence.first()
                    != evidence.viewport_chunk_sequence.last()
                || evidence.sampled_tile_count == 0
                || evidence.verified_page_load_count == 0
                || evidence.peak_resident_pages > super::MAX_RESIDENT_PAGES
            {
                return Err(SourceQualificationError::ReportFieldMismatch {
                    field: "source_workload_contracts.source_evidence",
                    left: format!("{evidence:?}"),
                    right: "valid source package and bounded six-position sparse viewport evidence"
                        .to_owned(),
                });
            }
            ensure_equal(
                "source_workload_contracts.source_evidence.tiles_per_side",
                evidence.tiles_per_side,
                contract.tiles_per_side,
            )?;
            ensure_equal(
                "source_workload_contracts.source_evidence.physical_side_meters",
                evidence.physical_side_meters,
                contract.physical_side_meters,
            )?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationVerdict {
    Passed,
    NotRun,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct QualificationSections {
    pub local_movement_endurance: QualificationVerdict,
    pub page_residency_churn: QualificationVerdict,
    pub resource_lifecycle: QualificationVerdict,
    pub geographic_long_distance_navigation: QualificationVerdict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutePlanningOutcome {
    Complete,
    Unreachable,
    SearchLimit,
    InvalidDestination,
    Cancelled,
    ProviderError,
}

#[derive(Clone, Debug, Serialize)]
pub struct RoutePlanningDiagnostic {
    pub case: &'static str,
    pub chain_leg: Option<u8>,
    pub origin: [i32; 2],
    pub destination: [i32; 2],
    pub outcome: RoutePlanningOutcome,
    pub work: u32,
    pub expansions: u32,
    pub peak_retained_entries: usize,
    pub path_tile_count: usize,
    pub work_limit: u32,
    pub node_limit: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectivityProbeOutcome {
    Connected,
    Disconnected,
    NodeLimit,
}

#[derive(Clone, Debug, Serialize)]
pub struct BoundedConnectivityDiagnostic {
    pub origin: [i32; 2],
    pub destination: [i32; 2],
    pub outcome: ConnectivityProbeOutcome,
    pub visited_tiles: usize,
    pub frontier_tiles_at_stop: usize,
    pub node_limit: usize,
    pub tested_parallel_corridor_radius_tiles: i32,
    pub tested_parallel_corridor_count: usize,
    pub connected_parallel_corridor_x_offset: Option<i32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceScaleEvidence {
    pub tiles_per_side: u64,
    pub physical_side_meters: u64,
    pub package_hash: String,
    pub generation_recipe_version: u16,
    pub sample_axis: u16,
    pub source_lock_count: usize,
    pub indexed_page_count: usize,
    pub viewport_chunk_sequence: Vec<[i32; 2]>,
    pub sampled_tile_count: usize,
    pub verified_page_load_count: u64,
    pub peak_resident_pages: usize,
    pub viewport_extent_tiles: [i32; 2],
}

#[derive(Clone, Debug, Serialize)]
pub struct GeographicNavigationEvidence {
    pub contract: &'static str,
    pub package_hash: String,
    pub map_center_latitude_e7: i32,
    pub map_center_longitude_e7: i32,
    pub start_tile: [i32; 2],
    /// Absolute destinations for the ordinary long move orders.
    pub route_waypoints: Vec<[i32; 2]>,
    pub long_order_count: usize,
    pub planned_distance_meters: f64,
    pub measured_distance_meters: f64,
    pub maximum_order_displacement_meters: f64,
    pub spatial_extent_tiles: [i32; 2],
    pub movement_ticks: u64,
    pub simulated_seconds: f64,
    pub measured_speed_meters_per_second: f64,
    pub configured_speed_meters_per_second: f64,
    pub speed_within_one_percent: bool,
    pub route_planner_work_total: u64,
    pub route_planner_work_max_order: u32,
    pub route_planner_expansions_total: u64,
    pub route_planner_peak_retained_entries: usize,
    pub peak_route_navigation_cache_entries: usize,
    pub peak_replay_navigation_cache_entries: usize,
    pub peak_combined_navigation_cache_entries: usize,
    pub peak_combined_navigation_cache_logical_retained_bytes: usize,
    pub route_planning_diagnostics: Vec<RoutePlanningDiagnostic>,
    /// The original northbound order remains a search-limit regression, distinct from the passing route.
    pub north_flat_route_diagnostic: RoutePlanningDiagnostic,
    pub north_connectivity_diagnostic: BoundedConnectivityDiagnostic,
    /// Makes repeated travel along one geographic corridor explicit.
    pub route_pattern: &'static str,
    pub unique_corridor_count: usize,
    /// Verified page payloads read on cache misses during order issue and travel.
    pub route_page_loads: u64,
    pub replay_page_loads: u64,
    pub peak_resident_pages_per_provider: usize,
    pub route_checkpoint_count: usize,
    pub replay_hash: String,
    pub replay_matches: bool,
}

pub(super) fn ensure_geographic_navigation_agreement(
    evidence: &GeographicNavigationEvidence,
    tick_hz: u32,
) -> Result<(), SourceQualificationError> {
    ensure_equal(
        "geographic_navigation.long_order_count",
        evidence.long_order_count,
        5,
    )?;
    ensure_equal(
        "geographic_navigation.route_waypoints.count",
        evidence.route_waypoints.len(),
        evidence.long_order_count,
    )?;
    ensure_equal(
        "geographic_navigation.route_planning_diagnostics.count",
        evidence.route_planning_diagnostics.len(),
        evidence.long_order_count,
    )?;
    if evidence.planned_distance_meters != route::REQUIRED_TRAVEL_METERS
        || evidence.measured_distance_meters < route::REQUIRED_TRAVEL_METERS
        || evidence.maximum_order_displacement_meters < 20_000.0
        || evidence.spatial_extent_tiles[0] < 10_000
        || evidence.spatial_extent_tiles[0] + evidence.spatial_extent_tiles[1] < 10_000
        || evidence.unique_corridor_count != 1
        || evidence.route_pattern
            != "five alternating ordinary orders over one 20km corridor; four repeat traversals"
        || evidence.route_planner_work_max_order > aoe_map::MAX_ROUTE_PLANNER_WORK
        || evidence.route_planner_peak_retained_entries > aoe_map::MAX_ROUTE_PLANNER_NODES
        || evidence.peak_combined_navigation_cache_logical_retained_bytes
            > MAX_NAVIGATION_CACHE_LOGICAL_BYTES
        || evidence.route_page_loads == 0
        || evidence.replay_page_loads == 0
        || evidence.peak_resident_pages_per_provider > super::MAX_RESIDENT_PAGES
        || !evidence.speed_within_one_percent
        || !evidence.replay_matches
    {
        return Err(SourceQualificationError::ReportFieldMismatch {
            field: "geographic_navigation.contract",
            left: format!("{evidence:?}"),
            right: "five completed 20 km orders along a disclosed repeated 20 km corridor, >=100 km measured travel, bounded planning/pages, replay and speed pass".to_owned(),
        });
    }
    ensure_exact_float(
        "geographic_navigation.simulated_seconds",
        evidence.simulated_seconds,
        evidence.movement_ticks as f64 / f64::from(tick_hz),
    )?;
    ensure_equal(
        "geographic_navigation.route_planning_diagnostics.outcomes",
        evidence
            .route_planning_diagnostics
            .iter()
            .all(|diagnostic| diagnostic.outcome == RoutePlanningOutcome::Complete),
        true,
    )?;
    ensure_equal(
        "geographic_navigation.north_flat_route_diagnostic.outcome",
        evidence.north_flat_route_diagnostic.outcome,
        RoutePlanningOutcome::SearchLimit,
    )?;
    ensure_equal(
        "geographic_navigation.north_flat_route_diagnostic.retained_cap",
        evidence.north_flat_route_diagnostic.peak_retained_entries,
        aoe_map::MAX_ROUTE_PLANNER_NODES,
    )?;
    ensure_equal(
        "geographic_navigation.north_connectivity_diagnostic.outcome",
        evidence.north_connectivity_diagnostic.outcome,
        ConnectivityProbeOutcome::NodeLimit,
    )?;
    ensure_equal(
        "geographic_navigation.replay_hash.length",
        evidence.replay_hash.len(),
        64,
    )
}
