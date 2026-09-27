use super::{Result, packages::CaptureInputs};
use std::{fs, path::Path};

pub(super) fn verify_capture(evidence: &Path, inputs: &CaptureInputs) -> Result<()> {
    let case = &inputs.eviction_case;
    let target = validate_case(case)?;
    for backend in ["webgpu", "canvas2d"] {
        let directory = evidence.join(&case.id);
        let metadata = directory.join(format!("{backend}.json"));
        let record: serde_json::Value = serde_json::from_slice(&fs::read(&metadata)?)?;
        verify_backend(&directory, &record, inputs, case, target, backend)?;
    }
    Ok(())
}

fn validate_case(case: &super::packages::CaptureCase) -> Result<&super::packages::EvictionTarget> {
    let target = case
        .eviction_target
        .as_ref()
        .ok_or("generated Alpine eviction case has no high-relief target")?;
    if case.id != "alpine_eviction"
        || case.tiles_per_side != 750
        || case.request.compression.numerator != 20
        || case.request.compression.denominator != 1
        || case.package_chunk_count_bound <= 512
        || case.package_chunk_count_bound != 576
        || case.page_count > 64
        || case.page_bytes > 1_048_576
        || case.source_locks.is_empty()
        || case
            .elevation_range_centimeters
            .maximum_centimeters
            .saturating_sub(case.elevation_range_centimeters.minimum_centimeters)
            < 150_000
        || target.chunk_x != 23
        || target.chunk_y != 23
        || target
            .maximum_elevation_centimeters
            .saturating_sub(target.minimum_elevation_centimeters)
            < 15_000
    {
        return Err("generated Alpine eviction package or southeast source relief did not meet its fixed bounds".into());
    }
    Ok(target)
}

fn verify_backend(
    directory: &Path,
    record: &serde_json::Value,
    inputs: &CaptureInputs,
    case: &super::packages::CaptureCase,
    target: &super::packages::EvictionTarget,
    backend: &str,
) -> Result<()> {
    for name in [
        format!("{backend}.png"),
        format!("{backend}-initial.png"),
        format!("{backend}-evicted.png"),
        format!("{backend}-returned.png"),
    ] {
        let image = directory.join(&name);
        if !image.is_file() || fs::metadata(&image)?.len() == 0 {
            return Err(format!("Alpine eviction {backend} screenshot {name} is missing").into());
        }
    }
    validate_backend_record(record, inputs, case, target, backend)
}

fn validate_backend_record(
    record: &serde_json::Value,
    inputs: &CaptureInputs,
    case: &super::packages::CaptureCase,
    target: &super::packages::EvictionTarget,
    backend: &str,
) -> Result<()> {
    let unique_requests = record
        .get("unique_chunk_request_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let cache_at_scan = record
        .get("cache_resident_chunks_after_scan")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(u64::MAX);
    let cache_after_return = record
        .get("cache_resident_chunks_after_return")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(u64::MAX);
    let target_requests_before_return = record
        .get("target_request_count_before_return")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let target_requests_after_return = record
        .get("target_request_count_after_return")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let loaded = record
        .get("loaded_chunks")
        .and_then(serde_json::Value::as_array);
    let expected_target_path = format!(
        "/maps/{}/chunks/{}/{}",
        case.content_hash, target.chunk_x, target.chunk_y
    );
    if record.get("case_id").and_then(serde_json::Value::as_str) != Some(case.id.as_str())
        || record
            .get("content_hash")
            .and_then(serde_json::Value::as_str)
            != Some(case.content_hash.as_str())
        || record.get("renderer").and_then(serde_json::Value::as_str) != Some(backend)
        || record
            .get("prepared_revision")
            .and_then(serde_json::Value::as_str)
            != Some(inputs.prepared_revision.as_str())
        || record
            .get("capture_revision")
            .and_then(serde_json::Value::as_str)
            != Some(inputs.capture_revision.as_str())
        || record
            .get("source_locks")
            .and_then(serde_json::Value::as_array)
            .is_none_or(Vec::is_empty)
        || record
            .pointer("/eviction_target/chunk_x")
            .and_then(serde_json::Value::as_u64)
            != Some(u64::from(target.chunk_x))
        || record
            .pointer("/eviction_target/chunk_y")
            .and_then(serde_json::Value::as_u64)
            != Some(u64::from(target.chunk_y))
        || unique_requests <= 512
        || cache_at_scan != 512
        || cache_after_return > 512
        || target_requests_after_return <= target_requests_before_return
        || loaded.is_none_or(|chunks| {
            chunks.len() != usize::try_from(unique_requests).unwrap_or(usize::MAX)
        })
        || record
            .get("target_chunk_path")
            .and_then(serde_json::Value::as_str)
            != Some(expected_target_path.as_str())
        || record
            .pointer("/page_evidence/southeast_chunk_elevation_centimeters/minimum")
            .and_then(serde_json::Value::as_i64)
            != Some(i64::from(target.minimum_elevation_centimeters))
        || record
            .pointer("/page_evidence/southeast_chunk_elevation_centimeters/maximum")
            .and_then(serde_json::Value::as_i64)
            != Some(i64::from(target.maximum_elevation_centimeters))
        || record
            .pointer("/interactions/eviction_exercised")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || record
            .pointer("/interactions/visual_restored")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || record
            .pointer("/interactions/visual_similarity_percent")
            .and_then(serde_json::Value::as_u64)
            .is_none_or(|percent| percent < 60)
    {
        return Err(format!(
            "Alpine {backend} eviction evidence did not prove source identity, bounded residency and target refetch"
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::e2e::visual::packages::{
        CaptureCase, ElevationRange, EvictionTarget, HistoricalLandUseEvidence,
    };

    fn case() -> CaptureCase {
        CaptureCase {
            id: "alpine_eviction".to_owned(),
            geometry: "relief".to_owned(),
            content_hash: "b".repeat(64),
            manifest_path: "fixture-only".to_owned(),
            location: "fixture-only".to_owned(),
            request: aoe_map::MapRequest {
                compression: aoe_map::Ratio {
                    numerator: 20,
                    denominator: 1,
                },
                ..aoe_map::MapRequest::default()
            },
            tiles_per_side: 750,
            package_chunk_count_bound: 576,
            schema_version: 9,
            generator_version: 1,
            generation_recipe_version: 1,
            source_locks: vec![aoe_map::SourceLock {
                id: "fixture".to_owned(),
                provider: "test-only".to_owned(),
                release: "fixture".to_owned(),
                url: "https://example.invalid/fixture".to_owned(),
                sha256: [1; 32],
                acquired_at: "2026-09-26T00:00:00Z".to_owned(),
                native_resolution: "test".to_owned(),
                crs: "EPSG:4326".to_owned(),
                vertical_datum: "test".to_owned(),
                license: "test-only".to_owned(),
                preprocessing_version: "fixture-v1".to_owned(),
            }],
            preparation_elapsed_milliseconds: 0,
            page_count: 64,
            page_bytes: 1_048_576,
            water_pages: None,
            historical_land_use: HistoricalLandUseEvidence::default(),
            elevation_range_centimeters: ElevationRange {
                minimum_centimeters: -100_000,
                maximum_centimeters: 100_000,
                level_zero_samples: 4,
            },
            eviction_target: Some(EvictionTarget {
                chunk_x: 23,
                chunk_y: 23,
                minimum_elevation_centimeters: 0,
                maximum_elevation_centimeters: 15_000,
            }),
        }
    }

    fn inputs(case: CaptureCase) -> CaptureInputs {
        CaptureInputs {
            version: 2,
            prepared_revision: "prepared-test-revision".to_owned(),
            capture_revision: "capture-test-revision".to_owned(),
            case_corrections: serde_json::Value::Null,
            activation_cases: Vec::new(),
            cases: Vec::new(),
            eviction_case: case,
        }
    }

    fn record(inputs: &CaptureInputs, case: &CaptureCase, backend: &str) -> serde_json::Value {
        let target = case.eviction_target.as_ref().unwrap();
        serde_json::json!({
            "case_id": case.id,
            "content_hash": case.content_hash,
            "renderer": backend,
            "prepared_revision": inputs.prepared_revision,
            "capture_revision": inputs.capture_revision,
            "source_locks": [{"fixture": true}],
            "eviction_target": {"chunk_x": target.chunk_x, "chunk_y": target.chunk_y},
            "unique_chunk_request_count": 513,
            "cache_resident_chunks_after_scan": 512,
            "cache_resident_chunks_after_return": 512,
            "target_request_count_before_return": 1,
            "target_request_count_after_return": 2,
            "loaded_chunks": vec!["fixture"; 513],
            "target_chunk_path": format!("/maps/{}/chunks/{}/{}", case.content_hash, target.chunk_x, target.chunk_y),
            "page_evidence": {"southeast_chunk_elevation_centimeters": {
                "minimum": target.minimum_elevation_centimeters,
                "maximum": target.maximum_elevation_centimeters
            }},
            "interactions": {
                "eviction_exercised": true,
                "visual_restored": true,
                "visual_similarity_percent": 60
            }
        })
    }

    // These fixtures call only the evidence contract validators. They do not
    // claim to be source-backed browser captures or qualify the renderer.
    #[test]
    fn eviction_contract_accepts_fixed_package_and_backend_bounds() {
        let case = case();
        let inputs = inputs(case.clone());
        let target = validate_case(&case).expect("fixed eviction package contract");
        let evidence = record(&inputs, &case, "webgpu");
        validate_backend_record(&evidence, &inputs, &case, target, "webgpu")
            .expect("well-formed bounded eviction record");
    }

    #[test]
    fn eviction_contract_rejects_cap_and_refetch_evidence_regressions() {
        let case = case();
        let inputs = inputs(case.clone());
        let target = validate_case(&case).unwrap();
        let mut malformed = record(&inputs, &case, "webgpu");
        malformed["cache_resident_chunks_after_scan"] = serde_json::json!(511);
        assert!(validate_backend_record(&malformed, &inputs, &case, target, "webgpu").is_err());
        let mut malformed = record(&inputs, &case, "webgpu");
        malformed["cache_resident_chunks_after_return"] = serde_json::json!(513);
        assert!(validate_backend_record(&malformed, &inputs, &case, target, "webgpu").is_err());
        let mut malformed = record(&inputs, &case, "webgpu");
        malformed["target_request_count_after_return"] = serde_json::json!(1);
        assert!(validate_backend_record(&malformed, &inputs, &case, target, "webgpu").is_err());
        let mut malformed = record(&inputs, &case, "webgpu");
        malformed["loaded_chunks"] = serde_json::json!([]);
        assert!(validate_backend_record(&malformed, &inputs, &case, target, "webgpu").is_err());
        let malformed_case = CaptureCase {
            page_bytes: 1_048_577,
            ..case
        };
        assert!(validate_case(&malformed_case).is_err());
    }
}
