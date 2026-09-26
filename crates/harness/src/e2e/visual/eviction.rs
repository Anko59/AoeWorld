use super::{Result, packages::CaptureInputs};
use std::{fs, path::Path};

pub(super) fn verify_capture(evidence: &Path, inputs: &CaptureInputs) -> Result<()> {
    let case = &inputs.eviction_case;
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
    for backend in ["webgpu", "canvas2d"] {
        let directory = evidence.join(&case.id);
        let metadata = directory.join(format!("{backend}.json"));
        let record: serde_json::Value = serde_json::from_slice(&fs::read(&metadata)?)?;
        verify_backend(&directory, &record, inputs, case, target, backend)?;
    }
    Ok(())
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
