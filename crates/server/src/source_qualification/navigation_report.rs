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
                || !super::supported_recipe(evidence.generation_recipe_version)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(outcome: RoutePlanningOutcome, leg: u8) -> RoutePlanningDiagnostic {
        RoutePlanningDiagnostic {
            case: "synthetic-route-fixture",
            chain_leg: Some(leg),
            origin: [10, 10],
            destination: [20, 10],
            outcome,
            work: 10,
            expansions: 2,
            peak_retained_entries: 8,
            path_tile_count: 11,
            work_limit: aoe_map::MAX_ROUTE_PLANNER_WORK,
            node_limit: aoe_map::MAX_ROUTE_PLANNER_NODES,
        }
    }

    fn valid_geographic_evidence() -> GeographicNavigationEvidence {
        GeographicNavigationEvidence {
            contract: "synthetic-offline-navigation-report-contract",
            package_hash: "a".repeat(64),
            map_center_latitude_e7: 250_000_000,
            map_center_longitude_e7: -50_000_000,
            start_tile: [24_999, 24_999],
            route_waypoints: vec![
                [34_999, 24_999],
                [24_999, 24_999],
                [34_999, 24_999],
                [24_999, 24_999],
                [34_999, 24_999],
            ],
            long_order_count: 5,
            planned_distance_meters: route::REQUIRED_TRAVEL_METERS,
            measured_distance_meters: route::REQUIRED_TRAVEL_METERS,
            maximum_order_displacement_meters: 20_000.0,
            spatial_extent_tiles: [10_000, 0],
            movement_ticks: 20,
            simulated_seconds: 1.0,
            measured_speed_meters_per_second: 3.0,
            configured_speed_meters_per_second: 3.0,
            speed_within_one_percent: true,
            route_planner_work_total: 50,
            route_planner_work_max_order: 10,
            route_planner_expansions_total: 25,
            route_planner_peak_retained_entries: 12,
            peak_route_navigation_cache_entries: 2,
            peak_replay_navigation_cache_entries: 2,
            peak_combined_navigation_cache_entries: 4,
            peak_combined_navigation_cache_logical_retained_bytes: 1_024,
            route_planning_diagnostics: (1..=5)
                .map(|leg| diagnostic(RoutePlanningOutcome::Complete, leg))
                .collect(),
            north_flat_route_diagnostic: RoutePlanningDiagnostic {
                peak_retained_entries: aoe_map::MAX_ROUTE_PLANNER_NODES,
                ..diagnostic(RoutePlanningOutcome::SearchLimit, 0)
            },
            north_connectivity_diagnostic: BoundedConnectivityDiagnostic {
                origin: [24_999, 24_999],
                destination: [24_999, 34_999],
                outcome: ConnectivityProbeOutcome::NodeLimit,
                visited_tiles: 10_000,
                frontier_tiles_at_stop: 100,
                node_limit: 10_000,
                tested_parallel_corridor_radius_tiles: 64,
                tested_parallel_corridor_count: 3,
                connected_parallel_corridor_x_offset: None,
            },
            route_pattern: "five alternating ordinary orders over one 20km corridor; four repeat traversals",
            unique_corridor_count: 1,
            route_page_loads: 10,
            replay_page_loads: 10,
            peak_resident_pages_per_provider: 16,
            route_checkpoint_count: 1,
            replay_hash: "b".repeat(64),
            replay_matches: true,
        }
    }

    fn scale_evidence(axis: u64) -> SourceScaleEvidence {
        SourceScaleEvidence {
            tiles_per_side: axis,
            physical_side_meters: axis * u64::from(aoe_map::GAME_TILE_METERS),
            package_hash: format!("{axis:064x}"),
            generation_recipe_version: aoe_map::GENERATION_RECIPE_VERSION,
            sample_axis: 64,
            source_lock_count: 1,
            indexed_page_count: 1,
            viewport_chunk_sequence: vec![[8, 8], [0, 0], [15, 0], [15, 15], [0, 15], [8, 8]],
            sampled_tile_count: 6,
            verified_page_load_count: 6,
            peak_resident_pages: 12,
            viewport_extent_tiles: [480, 480],
        }
    }

    #[test]
    fn offline_navigation_report_contract_accepts_complete_route_replay_and_bounded_caches() {
        ensure_geographic_navigation_agreement(&valid_geographic_evidence(), 20)
            .expect("well-formed report contract fixture");
    }

    #[test]
    fn navigation_report_rejects_route_replay_distance_and_shape_disagreements() {
        let mut evidence = valid_geographic_evidence();
        evidence.route_planning_diagnostics[2].outcome = RoutePlanningOutcome::Unreachable;
        assert!(matches!(
            ensure_geographic_navigation_agreement(&evidence, 20),
            Err(SourceQualificationError::ReportFieldMismatch { field, .. })
                if field == "geographic_navigation.route_planning_diagnostics.outcomes"
        ));

        let mut evidence = valid_geographic_evidence();
        evidence.route_page_loads = 0;
        assert!(matches!(
            ensure_geographic_navigation_agreement(&evidence, 20),
            Err(SourceQualificationError::ReportFieldMismatch { field, .. })
                if field == "geographic_navigation.contract"
        ));

        let mut evidence = valid_geographic_evidence();
        evidence.simulated_seconds = 2.0;
        assert!(matches!(
            ensure_geographic_navigation_agreement(&evidence, 20),
            Err(SourceQualificationError::ReportFieldMismatch { field, .. })
                if field == "geographic_navigation.simulated_seconds"
        ));

        let mut evidence = valid_geographic_evidence();
        evidence.replay_hash.pop();
        assert!(matches!(
            ensure_geographic_navigation_agreement(&evidence, 20),
            Err(SourceQualificationError::ReportFieldMismatch { field, .. })
                if field == "geographic_navigation.replay_hash.length"
        ));
    }

    #[test]
    fn scale_evidence_contract_checks_axes_source_identity_and_page_bounds() {
        let evidence = [512, 16_384, 50_000, 262_144]
            .into_iter()
            .map(scale_evidence)
            .collect::<Vec<_>>();
        let contracts = super::super::workload::source_workload_contracts(&evidence);
        ensure_scale_workload_agreement(&contracts).expect("valid source evidence fixtures");

        let mut wrong_axis = contracts.clone();
        wrong_axis[0].tiles_per_side = 513;
        assert!(matches!(
            ensure_scale_workload_agreement(&wrong_axis),
            Err(SourceQualificationError::ReportFieldMismatch { field, .. })
                if field == "source_workload_contracts.axes"
        ));

        let mut wrong_package_axis = contracts.clone();
        wrong_package_axis[0]
            .source_evidence
            .as_mut()
            .expect("source evidence")
            .tiles_per_side = 511;
        assert!(matches!(
            ensure_scale_workload_agreement(&wrong_package_axis),
            Err(SourceQualificationError::ReportFieldMismatch { field, .. })
                if field == "source_workload_contracts.source_evidence.tiles_per_side"
        ));

        let mut unbounded_pages = contracts;
        unbounded_pages[0]
            .source_evidence
            .as_mut()
            .expect("source evidence")
            .peak_resident_pages = super::super::MAX_RESIDENT_PAGES + 1;
        assert!(matches!(
            ensure_scale_workload_agreement(&unbounded_pages),
            Err(SourceQualificationError::ReportFieldMismatch { field, .. })
                if field == "source_workload_contracts.source_evidence"
        ));
    }
}
