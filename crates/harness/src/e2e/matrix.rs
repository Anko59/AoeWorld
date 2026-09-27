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
    validate_matrix_header(matrix.version, matrix.year_ce)?;
    let report_dir = root.join("reports/geodata/matrix");
    fs::create_dir_all(&report_dir)?;
    let package_dir = root.join("local-assets/maps-matrix");
    fs::create_dir_all(&package_dir)?;
    let worker_name = worker.to_str().ok_or("map worker path is not UTF-8")?;
    let cache_name = cache.to_str().ok_or("source cache path is not UTF-8")?;
    let mut outcomes = Vec::with_capacity(matrix.cases.len());
    for case in matrix.cases {
        validate_case(&case, matrix.year_ce)?;
        let request_path = report_dir.join(format!("{}.request.json", case.id));
        fs::write(&request_path, serde_json::to_vec_pretty(&case.request)?)?;
        let output = package_dir.join(&case.id);
        if output.exists() {
            fs::remove_dir_all(&output)?;
        }
        fs::create_dir_all(&output)?;
        let started = Instant::now();
        let prepared = prepare_case(
            worker_name,
            cache_name,
            &request_path,
            &output,
            case.request,
        );
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
    if report_result(&outcomes) != "PASS" {
        return Err(
            "geographic matrix incomplete or a case failed; see reports/geodata/matrix.json".into(),
        );
    }
    Ok(())
}

fn prepare_case(
    worker: &str,
    cache: &str,
    request_file: &Path,
    output: &Path,
    expected_request: MapRequest,
) -> Result<(String, MapPackage)> {
    let request_name = request_file.to_str().ok_or("request path is not UTF-8")?;
    let output_name = output.to_str().ok_or("output path is not UTF-8")?;
    process::run_with_env(
        worker,
        &["map-generate"],
        &[
            ("AOE_GEODATA_CACHE", cache),
            ("AOE_MAP_REQUEST", request_name),
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
    let stem = manifests[0]
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or("manifest filename is not UTF-8")?;
    let (hash, package) = validate_generated_package(expected_request, stem, package)?;
    let manifest_name = manifests[0].to_str().ok_or("manifest path is not UTF-8")?;
    process::run_with_env(
        worker,
        &["map-verify"],
        &[("AOE_MAP_PACKAGE", manifest_name)],
        Duration::from_secs(120),
    )?;
    Ok((hash, package))
}

fn validate_matrix_header(version: u16, year_ce: u16) -> Result<()> {
    if version != 1 || year_ce != 600 {
        return Err("unsupported geographic reference matrix version or year".into());
    }
    Ok(())
}

fn validate_case(case: &Case, matrix_year: u16) -> Result<()> {
    if case.id.is_empty()
        || !case
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err("matrix case ID must use lowercase ASCII, digits, and underscores".into());
    }
    if case.preparation != "overview" || case.request.year_ce != matrix_year {
        return Err(format!("unsupported matrix case: {}", case.id).into());
    }
    if case.request.normalized()? != case.request {
        return Err(format!("noncanonical matrix request: {}", case.id).into());
    }
    Ok(())
}

fn validate_generated_package(
    expected_request: MapRequest,
    manifest_stem: &str,
    package: MapPackage,
) -> Result<(String, MapPackage)> {
    package.validate()?;
    if package.request != expected_request {
        return Err("generated package request does not match the matrix case".into());
    }
    let hash = package.content_hash_hex();
    if package.source_locks.is_empty() || manifest_stem != hash {
        return Err("generated package is not canonically source-backed".into());
    }
    Ok((hash, package))
}

fn report_result(outcomes: &[Outcome]) -> &'static str {
    if outcomes.len() == 11 && outcomes.iter().all(|case| case.status == "PASS") {
        "PASS"
    } else {
        "FAIL"
    }
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
        "result": report_result(outcomes),
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

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_map::{MAP_SCHEMA_VERSION, SourceLock};

    fn request() -> MapRequest {
        MapRequest {
            year_ce: 600,
            ..MapRequest::default()
        }
    }

    fn package(request: MapRequest) -> MapPackage {
        let source_lock = SourceLock {
            id: "offline-fixture".to_owned(),
            provider: "test-only".to_owned(),
            release: "fixture".to_owned(),
            url: "https://example.invalid/fixture".to_owned(),
            sha256: [7; 32],
            acquired_at: "2026-09-26T00:00:00Z".to_owned(),
            native_resolution: "fixture".to_owned(),
            crs: "EPSG:4326".to_owned(),
            vertical_datum: "fixture".to_owned(),
            license: "test-only".to_owned(),
            preprocessing_version: "fixture-v1".to_owned(),
        };
        MapPackage::new(MAP_SCHEMA_VERSION, request, vec![source_lock])
            .expect("valid package fixture")
    }

    fn case(request: MapRequest) -> Case {
        Case {
            id: "sample_case".to_owned(),
            location: "fixture location".to_owned(),
            preparation: "overview".to_owned(),
            request,
        }
    }

    #[test]
    fn matrix_rejects_unsupported_version_and_year() {
        assert!(validate_matrix_header(1, 600).is_ok());
        assert!(validate_matrix_header(2, 600).is_err());
        assert!(validate_matrix_header(1, 601).is_err());
    }

    #[test]
    fn matrix_case_rejects_wrong_year_and_noncanonical_request() {
        assert!(validate_case(&case(request()), 600).is_ok());

        let mut wrong_year = case(request());
        wrong_year.request.year_ce = 601;
        assert!(validate_case(&wrong_year, 600).is_err());

        let mut noncanonical = case(request());
        noncanonical.request.center_longitude_e7 = 1_810_000_000;
        assert!(validate_case(&noncanonical, 600).is_err());

        let mut invalid_id = case(request());
        invalid_id.id = "Bad-ID".to_owned();
        assert!(validate_case(&invalid_id, 600).is_err());

        let mut wrong_profile = case(request());
        wrong_profile.preparation = "detailed".to_owned();
        assert!(validate_case(&wrong_profile, 600).is_err());
    }

    #[test]
    fn generated_package_must_match_request_hash_and_source_lock() {
        let expected = request();
        let valid = package(expected);
        let hash = valid.content_hash_hex();
        assert!(validate_generated_package(expected, &hash, valid.clone()).is_ok());

        let wrong_request = MapRequest {
            seed: expected.seed + 1,
            ..expected
        };
        assert!(validate_generated_package(expected, &hash, package(wrong_request)).is_err());
        assert!(validate_generated_package(expected, &"0".repeat(64), valid.clone()).is_err());

        let unlocked = MapPackage::new(MAP_SCHEMA_VERSION, expected, Vec::new())
            .expect("valid unlocked package fixture");
        assert!(
            validate_generated_package(expected, &unlocked.content_hash_hex(), unlocked).is_err()
        );

        let mut bad_hash = valid;
        bad_hash.content_hash[0] ^= 1;
        assert!(validate_generated_package(expected, &hash, bad_hash).is_err());
    }

    #[test]
    fn partial_matrix_success_is_reported_as_failure() {
        let outcomes = [
            Outcome {
                id: "first".to_owned(),
                location: "fixture".to_owned(),
                request: request(),
                elapsed_milliseconds: 1,
                status: "PASS",
                error: None,
                content_hash: Some("fixture-hash".to_owned()),
                package: None,
            },
            Outcome {
                id: "second".to_owned(),
                location: "fixture".to_owned(),
                request: request(),
                elapsed_milliseconds: 1,
                status: "FAIL",
                error: Some("fixture failure".to_owned()),
                content_hash: None,
                package: None,
            },
        ];
        assert_eq!(report_result(&outcomes), "FAIL");
    }
}
