use super::*;

#[test]
fn route_evidence_records_local_extent_repetitions_ticks_distance_and_hash() {
    let json = serde_json::to_value(valid_report().route_evidence).expect("route evidence JSON");

    assert_eq!(json["spatial_extent_tiles"], serde_json::json!([2, 0]));
    assert_eq!(json["leg_length_meters"], 4.0);
    assert_eq!(json["repetition_count"], 25_000);
    assert_eq!(json["movement_ticks"], 666_667);
    assert_eq!(json["moved_meters"], 100_000.0);
    assert_eq!(json["replay_hash"].as_str().expect("hash").len(), 64);
    assert_eq!(json["replay_matches"], true);
}

#[test]
fn complete_report_cross_sections_agree() {
    ensure_report_agreement(&valid_report(), 20).expect("complete report");
}

#[test]
fn report_agreement_rejects_cross_section_mismatches() {
    let mut ticks = valid_report();
    ticks.route_evidence.movement_ticks += 1;
    assert!(matches!(
        ensure_report_agreement(&ticks, 20),
        Err(SourceQualificationError::ReportFieldMismatch {
            field: "route_evidence.movement_ticks",
            ..
        })
    ));

    let mut hash = valid_report();
    hash.route_evidence.replay_hash = "0".repeat(63);
    hash.replay_hash = hash.route_evidence.replay_hash.clone();
    assert!(matches!(
        ensure_report_agreement(&hash, 20),
        Err(SourceQualificationError::ReportFieldMismatch {
            field: "route_evidence.replay_hash.length",
            ..
        })
    ));

    let mut extent = valid_report();
    extent.route_evidence.spatial_extent_tiles = [3, 0];
    assert!(matches!(
        ensure_report_agreement(&extent, 20),
        Err(SourceQualificationError::ReportFieldMismatch {
            field: "route_evidence.spatial_extent_tiles",
            ..
        })
    ));

    let mut seconds = valid_report();
    seconds.simulated_seconds += 1.0;
    assert!(matches!(
        ensure_report_agreement(&seconds, 20),
        Err(SourceQualificationError::ReportFieldMismatch {
            field: "simulated_seconds",
            ..
        })
    ));
}

fn valid_report() -> SourceQualificationReport {
    let replay_hash = "7".repeat(64);
    let route_evidence = RouteEvidence {
        contract: "fixed-cardinal-repeat-within-ordinary-component-v1",
        spatial_scope: "ordinary_activation_component_local",
        start_tile: [10, 10],
        alternate_tile: [12, 10],
        offset_tiles: [2, 0],
        spatial_extent_tiles: [2, 0],
        leg_length_tiles: 2,
        leg_length_meters: 4.0,
        repetition_count: 25_000,
        required_distance_meters: 100_000.0,
        movement_ticks: 666_667,
        moved_meters: 100_000.0,
        replay_hash: replay_hash.clone(),
        replay_matches: true,
    };
    let resource_lifecycle = ResourceLifecycleEvidence {
        initial_snapshot_count: 1,
        resource_mutation_count: 1,
        delta_snapshot_count: 1,
        resume_reconnect_snapshot_count: 1,
        restart_count: 1,
        persisted_snapshot_replay_count: 1,
        post_restart_client_snapshot_count: 1,
        page_residency_recreation_count: 1,
        stored_map_recreation_count: 1,
        network_connection_count: 3,
        resume_token_reconnect_verified: true,
        changed_world_id_verified: true,
        persisted_snapshot_verified: true,
        post_restart_client_snapshot_verified: true,
        network_resume_snapshot_verified: true,
        network_post_restart_snapshot_verified: true,
    };
    SourceQualificationReport {
        qualification_case: "source-backed-100km-50k-tiles-1-to-1",
        package_hash: "a".repeat(64),
        schema_version: 9,
        generator_version: 9,
        generation_recipe_version: 5,
        tiles_per_side: 50_000,
        physical_side_meters: 100_000,
        sample_axis: 1_024,
        source_lock_count: 1,
        source_lock_ids: vec!["fixture".to_owned()],
        start_tile: [10, 10],
        activation_component_policy: "ordinary-bounded-component-diagnostic-v1",
        activation_component_diagnostic: "fixture".to_owned(),
        route_waypoints: vec![[10, 10], [12, 10]],
        route_evidence,
        movement_ticks: 666_667,
        simulated_seconds: 33_333.35,
        moved_meters: 100_000.0,
        wall_seconds: 1.0,
        replay_hash: replay_hash.clone(),
        replay_matches: true,
        route_checkpoint_count: 81,
        indexed_page_count: 1_900,
        page_churn_unique_count: 1_900,
        peak_resident_page_count_per_provider: 128,
        evicted_page_reloaded_same_hash: true,
        resource_id: 1,
        resource_tile: [9, 9],
        resource_overlay_revision: 1,
        resource_overlay_reloaded_equal: true,
        logical_memory: LogicalMemoryEvidence {
            indexed_page_count: 1_900,
            page_churn_count: 1_900,
            peak_resident_pages_per_provider: 128,
            peak_route_navigation_cache_entries: 0,
            peak_replay_navigation_cache_entries: 0,
            peak_combined_navigation_cache_entries: 0,
            peak_route_navigation_cache_logical_retained_bytes: 0,
            peak_replay_navigation_cache_logical_retained_bytes: 0,
            peak_combined_navigation_cache_logical_retained_bytes: 0,
            navigation_cache_byte_scope: "logical payload only; excludes allocator and map container overhead",
            resource_overlay_change_count: 1,
        },
        process_rss_bytes: ProcessRssBytes {
            start: Some(1),
            peak: Some(2),
            end: Some(1),
        },
        simulation_work: SimulationWork {
            movement_ticks: 666_667,
            simulated_seconds: 33_333.35,
            route_movement_leg_count: 25_000,
            replay_movement_leg_count: 25_000,
            route_repetition_count: 25_000,
            route_moved_meters: 100_000.0,
            replay_moved_meters: 100_000.0,
            route_checkpoint_count: 81,
            movement_replay_comparison_count: 83,
            resource_lifecycle,
        },
        source_workload_contracts: Vec::new(),
    }
}
