use super::*;

fn case() -> packages::CaptureCase {
    packages::CaptureCase {
        id: "river_lake".to_owned(),
        geometry: "inland".to_owned(),
        content_hash: "a".repeat(64),
        manifest_path: "fixture-only".to_owned(),
        location: "fixture-only".to_owned(),
        request: aoe_map::MapRequest::default(),
        tiles_per_side: 64,
        package_chunk_count_bound: 4,
        schema_version: aoe_map::MAP_SCHEMA_VERSION,
        generator_version: 1,
        generation_recipe_version: 1,
        source_locks: Vec::new(),
        preparation_elapsed_milliseconds: 15,
        page_count: 3,
        page_bytes: 99,
        water_pages: None,
        historical_land_use: packages::HistoricalLandUseEvidence::default(),
        elevation_range_centimeters: packages::ElevationRange {
            minimum_centimeters: 0,
            maximum_centimeters: 200_000,
            level_zero_samples: 4,
        },
        eviction_target: None,
    }
}

fn inputs(case: packages::CaptureCase) -> CaptureInputs {
    CaptureInputs {
        version: 2,
        prepared_revision: "prepared-test-revision".to_owned(),
        capture_revision: "capture-test-revision".to_owned(),
        case_corrections: serde_json::Value::Null,
        activation_cases: vec![case.clone()],
        cases: vec![case.clone()],
        eviction_case: case,
    }
}

fn activation_record(case: &packages::CaptureCase, active: bool) -> serde_json::Value {
    let mut record = serde_json::json!({
        "case_id": case.id,
        "content_hash": case.content_hash,
        "preparation_elapsed_milliseconds": case.preparation_elapsed_milliseconds,
        "page_count": case.page_count,
        "page_bytes": case.page_bytes,
        "package_chunk_count_bound": case.package_chunk_count_bound,
        "start_available": active,
    });
    if active {
        record["loaded_chunks"] = serde_json::json!(["0/0", "1/0"]);
        record["loaded_chunk_count"] = serde_json::json!(2);
    } else {
        record["result"] = serde_json::json!("uninhabitable_preview_only");
        record["no_capture_reason"] = serde_json::json!("test fixture has no playable start");
    }
    record
}

fn capture_record(inputs: &CaptureInputs, case: &packages::CaptureCase) -> serde_json::Value {
    serde_json::json!({
        "content_hash": case.content_hash,
        "renderer": "webgpu",
        "prepared_revision": inputs.prepared_revision,
        "capture_revision": inputs.capture_revision,
        "source_locks": [{"fixture": true}],
        "loaded_chunks": ["0/0", "1/0"],
        "interactions": {
            "panned": true,
            "zoomed": true,
            "reconnected": true,
            "reloaded": true
        }
    })
}

// These offline values exercise only the JSON contract validators; they do not
// produce or stand in for a source-backed browser qualification report.
#[test]
fn report_contract_accepts_active_and_preview_only_outcomes() {
    let case = case();
    assert!(validate_activation_outcome(&case, &activation_record(&case, true)).unwrap());
    assert!(!validate_activation_outcome(&case, &activation_record(&case, false)).unwrap());

    let inputs = inputs(case.clone());
    validate_capture_record(&case, &inputs, &capture_record(&inputs, &case), "webgpu")
        .expect("well-formed capture metadata contract");
}

#[test]
fn report_contract_rejects_wrong_identity_missing_evidence_and_work_over_bound() {
    let case = case();
    let mut activation = activation_record(&case, true);
    activation["page_count"] = serde_json::json!(case.page_count + 1);
    assert!(validate_activation_outcome(&case, &activation).is_err());
    let mut activation = activation_record(&case, true);
    activation["loaded_chunk_count"] = serde_json::json!(5);
    activation["loaded_chunks"] = serde_json::json!(["0/0", "1/0", "2/0", "3/0", "4/0"]);
    assert!(validate_activation_outcome(&case, &activation).is_err());
    let mut preview = activation_record(&case, false);
    preview["no_capture_reason"] = serde_json::json!("");
    assert!(validate_activation_outcome(&case, &preview).is_err());

    let inputs = inputs(case.clone());
    let mut capture = capture_record(&inputs, &case);
    capture["prepared_revision"] = serde_json::json!("other-revision");
    assert!(validate_capture_record(&case, &inputs, &capture, "webgpu").is_err());
    let mut capture = capture_record(&inputs, &case);
    capture["loaded_chunks"] = serde_json::json!([]);
    assert!(validate_capture_record(&case, &inputs, &capture, "webgpu").is_err());
    let mut capture = capture_record(&inputs, &case);
    capture["source_locks"] = serde_json::json!([]);
    assert!(validate_capture_record(&case, &inputs, &capture, "webgpu").is_err());
    let mut capture = capture_record(&inputs, &case);
    capture["interactions"]["reloaded"] = serde_json::json!(false);
    assert!(validate_capture_record(&case, &inputs, &capture, "webgpu").is_err());
}
