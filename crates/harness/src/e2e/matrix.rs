//! Fixed geographic requests, prepared by the ordinary native worker.
use crate::process;
use aoe_map::{MapPackage, MapRequest};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
struct Matrix {
    version: u16,
    year_ce: u16,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    location: String,
    preparation: String,
    request: MapRequest,
}

#[derive(Serialize)]
struct Outcome {
    id: String,
    location: String,
    request: MapRequest,
    elapsed_milliseconds: u64,
    status: &'static str,
    error: Option<String>,
    content_hash: Option<String>,
    package: Option<MapPackage>,
}

pub(super) fn run() -> Result<()> {
    let root = env::current_dir()?.canonicalize()?;
    let cache = PathBuf::from(
        env::var_os("AOE_GEODATA_CACHE")
            .ok_or("test-geographic-matrix requires AOE_GEODATA_CACHE")?,
    )
    .canonicalize()?;
    if !cache.is_dir() {
        return Err("AOE_GEODATA_CACHE must be a directory".into());
    }
    let worker = root.join("target/release/aoe-map-worker");
    if !worker.is_file() {
        return Err(format!("map worker is missing: {}", worker.display()).into());
    }
    let matrix: Matrix =
        serde_json::from_slice(&fs::read(root.join("docs/geodata/reference-matrix.json"))?)?;
    if matrix.version != 1 || matrix.year_ce != 600 {
        return Err("unsupported geographic reference matrix version or year".into());
    }
    let report_dir = root.join("reports/geodata/matrix");
    fs::create_dir_all(&report_dir)?;
    let package_dir = root.join("local-assets/maps-matrix");
    fs::create_dir_all(&package_dir)?;
    let worker_name = worker.to_str().ok_or("map worker path is not UTF-8")?;
    let cache_name = cache.to_str().ok_or("source cache path is not UTF-8")?;
    let mut outcomes = Vec::with_capacity(matrix.cases.len());
    for case in matrix.cases {
        if case.id.is_empty()
            || !case
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err("matrix case ID must use lowercase ASCII, digits, and underscores".into());
        }
        if case.preparation != "overview" || case.request.year_ce != matrix.year_ce {
            return Err(format!("unsupported matrix case: {}", case.id).into());
        }
        if case.request.normalized()? != case.request {
            return Err(format!("noncanonical matrix request: {}", case.id).into());
        }
        let request_path = report_dir.join(format!("{}.request.json", case.id));
        fs::write(&request_path, serde_json::to_vec_pretty(&case.request)?)?;
        let output = package_dir.join(&case.id);
        if output.exists() {
            fs::remove_dir_all(&output)?;
        }
        fs::create_dir_all(&output)?;
        let started = Instant::now();
        let prepared = prepare_case(worker_name, cache_name, &request_path, &output);
        let elapsed_milliseconds = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let outcome = match prepared {
            Ok((hash, package)) => Outcome {
                id: case.id,
                location: case.location,
                request: case.request,
                elapsed_milliseconds,
                status: "PASS",
                error: None,
                content_hash: Some(hash),
                package: Some(package),
            },
            Err(error) => Outcome {
                id: case.id,
                location: case.location,
                request: case.request,
                elapsed_milliseconds,
                status: "FAIL",
                error: Some(error.to_string()),
                content_hash: None,
                package: None,
            },
        };
        println!(
            "{}: {} ({elapsed_milliseconds} ms)",
            outcome.id, outcome.status
        );
        outcomes.push(outcome);
        write_report(&root, &outcomes)?;
    }
    if outcomes.iter().any(|outcome| outcome.status != "PASS") {
        return Err(
            "one or more geographic matrix cases failed; see reports/geodata/matrix.json".into(),
        );
    }
    Ok(())
}

fn prepare_case(
    worker: &str,
    cache: &str,
    request: &Path,
    output: &Path,
) -> Result<(String, MapPackage)> {
    let request = request.to_str().ok_or("request path is not UTF-8")?;
    let output_name = output.to_str().ok_or("output path is not UTF-8")?;
    process::run_with_env(
        worker,
        &["map-generate"],
        &[
            ("AOE_GEODATA_CACHE", cache),
            ("AOE_MAP_REQUEST", request),
            ("AOE_MAP_PACKAGE", output_name),
        ],
        Duration::from_secs(900),
    )?;
    let mut manifests = fs::read_dir(output)?
        .map(|entry| entry.map(|value| value.path()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    manifests.retain(|path| path.extension().is_some_and(|ext| ext == "json"));
    if manifests.len() != 1 {
        return Err(format!("expected one manifest in {}", output.display()).into());
    }
    let package: MapPackage = serde_json::from_slice(&fs::read(&manifests[0])?)?;
    package.validate()?;
    let hash = package.content_hash_hex();
    if package.source_locks.is_empty()
        || manifests[0].file_stem().and_then(|s| s.to_str()) != Some(&hash)
    {
        return Err("generated package is not canonically source-backed".into());
    }
    let manifest_name = manifests[0].to_str().ok_or("manifest path is not UTF-8")?;
    process::run_with_env(
        worker,
        &["map-verify"],
        &[("AOE_MAP_PACKAGE", manifest_name)],
        Duration::from_secs(120),
    )?;
    Ok((hash, package))
}

fn write_report(root: &Path, outcomes: &[Outcome]) -> Result<()> {
    let revision = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    if !revision.status.success() {
        return Err("cannot identify geographic matrix revision".into());
    }
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()?;
    if !dirty.status.success() {
        return Err("cannot identify geographic matrix working tree".into());
    }
    let report = serde_json::json!({
        "version": 1,
        "revision": String::from_utf8(revision.stdout)?.trim(),
        "dirty": !dirty.stdout.is_empty(),
        "result": if outcomes.len() == 11 && outcomes.iter().all(|case| case.status == "PASS") { "PASS" } else { "FAIL" },
        "cases_attempted": outcomes.len(),
        "cases": outcomes,
        "limits": {
            "preparation": "128-axis source-backed overview",
            "memory_and_work": "not measured by this runner",
            "activation_and_visuals": "not qualified by this runner; the Paris creator journey is separate"
        }
    });
    fs::write(
        root.join("reports/geodata/matrix.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}
