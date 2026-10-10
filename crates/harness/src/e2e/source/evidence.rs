//! Validates creator evidence without starting a browser or source worker.
use super::{MapPackage, MapRequest, Result, evidence_hash};

pub(super) fn created_record(
    profile: &str,
    package: &MapPackage,
    created_evidence: &serde_json::Value,
    paris_request: MapRequest,
) -> Result<serde_json::Value> {
    if !super::CREATOR_PROFILES.contains(&profile) {
        return Err("unknown creator profile".into());
    }
    package.validate()?;
    let hash = evidence_hash(created_evidence)?;
    if package.content_hash_hex() != hash
        || package.source_locks.is_empty()
        || package.request != paris_request
    {
        return Err(format!(
            "{profile} creator did not publish the fixed source-backed Paris package"
        )
        .into());
    }
    let expected_axis = if profile == "detailed" { 1_024 } else { 128 };
    let model_present = package
        .environment
        .hydrology_evidence
        .as_ref()
        .and_then(|evidence| evidence.water_model.as_ref())
        .is_some();
    if package.environment.samples_per_axis != expected_axis
        || package.environment.elevation.levels.is_empty()
        || created_evidence
            .get("preparation")
            .and_then(|value| value.get("mode"))
            .and_then(serde_json::Value::as_str)
            != Some(profile)
    {
        return Err(
            format!("{profile} creator did not retain its requested preparation grid").into(),
        );
    }
    if profile == "detailed"
        && (package.generation_recipe_version != aoe_map::GENERATION_RECIPE_VERSION
            || !model_present)
    {
        return Err(
            "detailed Paris package lacks current-recipe modeled-water source pages".into(),
        );
    }
    if profile == "overview"
        && (package.generation_recipe_version != aoe_map::GENERATION_RECIPE_VERSION
            || model_present)
    {
        return Err(
            "overview Paris package did not retain its model-free current-recipe identity".into(),
        );
    }
    Ok(serde_json::json!({
        "profile": profile,
        "content_hash": hash,
        "generation_recipe_version": package.generation_recipe_version,
        "modeled_water_source_model": model_present,
        "samples_per_axis": package.environment.samples_per_axis,
        "source_locks": package.source_locks,
        "projection": package.projection,
        "provenance": package.provenance,
        "environment": package.environment,
        "evidence": created_evidence,
    }))
}

pub(super) fn reopen_record(
    profile: &str,
    created_case: &serde_json::Value,
    reopened: &serde_json::Value,
) -> Result<serde_json::Value> {
    if !super::CREATOR_PROFILES.contains(&profile) {
        return Err("unknown creator profile".into());
    }
    let hash = created_case["content_hash"]
        .as_str()
        .ok_or("creator result lacks content hash")?;
    let unseen_chunk = reopened
        .get("unseen_chunk")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if created_case["profile"].as_str() != Some(profile)
        || !valid_chunk_path(unseen_chunk, hash)
        || created_case["evidence"]["joined_world_chunks"]
            .as_array()
            .is_some_and(|chunks| {
                chunks
                    .iter()
                    .any(|chunk| chunk.as_str() == Some(unseen_chunk))
            })
        || evidence_hash(reopened)? != hash
        || reopened
            .get("offline_unseen_chunk")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || !has_joined_package_chunk(&created_case["evidence"], hash)
        || !has_joined_package_chunk(reopened, hash)
        || reopened
            .get("joined_world_chunks")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|chunks| {
                chunks
                    .iter()
                    .any(|chunk| chunk.as_str() == Some(unseen_chunk))
            })
    {
        return Err(format!("{profile} source-backed reopen did not join the same offline package and load a separate unseen chunk").into());
    }
    Ok(serde_json::json!({
        "version": 1,
        "result": "PASS",
        "profile": profile,
        "content_hash": hash,
        "generation_recipe_version": created_case["generation_recipe_version"],
        "modeled_water_source_model": created_case["modeled_water_source_model"],
        "samples_per_axis": created_case["samples_per_axis"],
        "source_locks": created_case["source_locks"],
        "projection": created_case["projection"],
        "provenance": created_case["provenance"],
        "environment": created_case["environment"],
        "created": created_case["evidence"],
        "reopened": reopened,
        "offline_policy": "server restarted without a map worker and with a freshly emptied source cache",
    }))
}

fn has_joined_package_chunk(value: &serde_json::Value, hash: &str) -> bool {
    value
        .get("joined_world_chunks")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|chunks| {
            chunks
                .iter()
                .filter_map(serde_json::Value::as_str)
                .any(|path| valid_chunk_path(path, hash))
        })
}

fn valid_chunk_path(path: &str, hash: &str) -> bool {
    let prefix = format!("/maps/{hash}/chunks/");
    let Some(suffix) = path.strip_prefix(&prefix) else {
        return false;
    };
    let Some((x, y)) = suffix.split_once('/') else {
        return false;
    };
    !x.is_empty()
        && !y.is_empty()
        && x.bytes().all(|byte| byte.is_ascii_digit())
        && y.bytes().all(|byte| byte.is_ascii_digit())
        && x.parse::<u32>().is_ok()
        && y.parse::<u32>().is_ok()
}

#[cfg(test)]
mod tests;
