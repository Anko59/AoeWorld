use super::*;
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage};
use std::{fs, os::unix::fs::PermissionsExt};

const REQUEST: &str = r#"{"schema_version":1,"center_latitude_e7":390000000,"center_longitude_e7":340000000,"requested_side_meters":960000,"compression":{"numerator":240,"denominator":1},"year_ce":600,"seed":1,"reconstruction_profile":"circa600_v1"}"#;

fn request() -> MapRequest {
    parse_request(REQUEST.as_bytes()).unwrap()
}

/// A package manifest and its page directory, as the generator leaves them.
fn publish_package(directory: &Path, request: MapRequest) -> String {
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).unwrap();
    let hash = package.content_hash_hex();
    fs::create_dir_all(directory.join("pages").join(&hash)).unwrap();
    fs::write(
        directory.join(format!("{hash}.json")),
        serde_json::to_vec(&package).unwrap(),
    )
    .unwrap();
    hash
}

/// A worker that logs each command to `calls` and runs `script` for it.
fn fake_worker(root: &Path, script: &str) -> PathBuf {
    let path = root.join("worker.sh");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\necho \"$1\" >> '{}/calls'\n{script}\n",
            root.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn calls(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn options(root: &Path, worker: PathBuf) -> Options {
    let request_path = root.join("request.json");
    fs::write(&request_path, REQUEST).unwrap();
    Options {
        request: request_path,
        cache: root.join("geodata"),
        packages: Some(root.join("packages")),
        worker,
        attempts: 2,
        retry_wait_seconds: 0,
        no_fetch: false,
        soft: false,
    }
}

#[test]
fn the_committed_request_is_the_owners_anatolia_map() {
    let text =
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../qa/anatolia-map.json")).unwrap();
    let request = parse_request(&text).unwrap();
    assert_eq!(request.center_latitude_e7, 390_000_000);
    assert_eq!(request.center_longitude_e7, 340_000_000);
    assert_eq!(request.requested_side_meters, 960_000);
    assert_eq!(
        (
            request.compression.numerator,
            request.compression.denominator
        ),
        (240, 1)
    );
    assert_eq!((request.year_ce, request.seed), (600, 1));
}

#[test]
fn a_request_that_does_not_parse_or_normalize_is_refused_with_its_reason() {
    assert!(parse_request(b"{").is_err());
    let bad = REQUEST.replace("\"year_ce\":600", "\"year_ce\":1900");
    assert!(parse_request(bad.as_bytes()).is_err());
}

#[test]
fn the_cache_key_names_the_request_and_nothing_else() {
    let key = cache_key(&request());
    assert!(key.starts_with("game-map-"), "{key}");
    assert_eq!(key.len(), "game-map-".len() + 16);
    assert_eq!(key, cache_key(&request()));
    let mut other = request();
    other.seed = 2;
    assert_ne!(key, cache_key(&other));
    other = request();
    other.requested_side_meters = 2_160_000;
    assert_ne!(key, cache_key(&other));
}

#[test]
fn an_existing_package_for_the_request_is_reused_without_running_the_worker() {
    let temp = tempfile::tempdir().unwrap();
    let worker = fake_worker(temp.path(), "exit 1");
    let options = options(temp.path(), worker);
    let hash = publish_package(&options.packages(), request());
    match ensure(&options).unwrap() {
        Outcome::Ready(ready) => {
            assert_eq!(ready.hash, hash);
            assert!(!ready.generated);
            assert_eq!(
                ready.manifest,
                options.packages().join(format!("{hash}.json"))
            );
        }
        Outcome::Unavailable(reason) => panic!("{reason}"),
    }
    assert!(calls(temp.path()).is_empty());
}

#[test]
fn a_package_of_another_request_is_not_taken_for_this_one() {
    let temp = tempfile::tempdir().unwrap();
    let worker = fake_worker(temp.path(), "exit 1");
    let mut options = options(temp.path(), worker);
    options.no_fetch = true;
    let mut other = request();
    other.seed = 7;
    publish_package(&options.packages(), other);
    assert!(matches!(ensure(&options).unwrap(), Outcome::Unavailable(_)));
}

#[test]
fn a_manifest_without_its_pages_is_not_a_package() {
    let temp = tempfile::tempdir().unwrap();
    let worker = fake_worker(temp.path(), "exit 1");
    let mut options = options(temp.path(), worker);
    options.no_fetch = true;
    let hash = publish_package(&options.packages(), request());
    fs::remove_dir_all(options.packages().join("pages").join(hash)).unwrap();
    assert!(matches!(ensure(&options).unwrap(), Outcome::Unavailable(_)));
}

#[test]
fn no_fetch_with_no_package_says_so_and_never_runs_the_worker() {
    let temp = tempfile::tempdir().unwrap();
    let worker = fake_worker(temp.path(), "exit 0");
    let mut options = options(temp.path(), worker);
    options.no_fetch = true;
    let Outcome::Unavailable(reason) = ensure(&options).unwrap() else {
        panic!("no package can exist");
    };
    assert!(reason.starts_with("map unavailable"), "{reason}");
    assert!(reason.contains(&cache_key(&request())), "{reason}");
    assert!(reason.contains("--no-fetch"), "{reason}");
    assert!(calls(temp.path()).is_empty());
}

#[test]
fn missing_sources_are_named_in_the_failure_after_the_retries() {
    let temp = tempfile::tempdir().unwrap();
    let worker = fake_worker(
        temp.path(),
        "echo 'source acquisition failed: etopo-2022-v1-60s-surface' >&2; exit 1",
    );
    let Outcome::Unavailable(reason) = ensure(&options(temp.path(), worker)).unwrap() else {
        panic!("no source can be fetched");
    };
    assert!(reason.starts_with("map unavailable"), "{reason}");
    assert!(reason.contains("etopo-2022-v1-60s-surface"), "{reason}");
}

#[test]
fn a_cold_cache_bootstraps_with_retries_then_generates_and_verifies() {
    let temp = tempfile::tempdir().unwrap();
    // The first bootstrap fails the way a dropped connection does; the second works.
    let script = format!(
        "case \"$1\" in\n bootstrap) n=$(grep -c bootstrap '{root}/calls'); [ \"$n\" -ge 2 ] || {{ echo 'connection reset' >&2; exit 1; }} ;;\n map-generate) cp -R '{root}/staged/.' '{root}/packages/' ;;\n esac",
        root = temp.path().display()
    );
    let worker = fake_worker(temp.path(), &script);
    let options = options(temp.path(), worker);
    let staged = temp.path().join("staged");
    let hash = publish_package(&staged, request());
    fs::create_dir_all(options.packages()).unwrap();
    let Outcome::Ready(ready) = ensure(&options).unwrap() else {
        panic!("generation succeeds on the second bootstrap");
    };
    assert_eq!(ready.hash, hash);
    assert!(ready.generated);
    assert_eq!(
        calls(temp.path()),
        ["bootstrap", "bootstrap", "map-generate", "map-verify"]
    );
}

#[test]
fn a_network_failure_that_outlasts_the_retries_is_unavailable_not_a_crash() {
    let temp = tempfile::tempdir().unwrap();
    let worker = fake_worker(temp.path(), "echo 'dns error' >&2; exit 1");
    let options = options(temp.path(), worker);
    let Outcome::Unavailable(reason) = ensure(&options).unwrap() else {
        panic!("bootstrap never works");
    };
    assert!(reason.contains("dns error"), "{reason}");
    assert!(reason.contains("2 attempts"), "{reason}");
    assert_eq!(calls(temp.path()), ["bootstrap", "bootstrap"]);
}

#[test]
fn soft_mode_prints_the_reason_and_leaves_a_marker_for_the_later_job() {
    let temp = tempfile::tempdir().unwrap();
    let worker = fake_worker(temp.path(), "exit 1");
    let mut options = options(temp.path(), worker);
    options.no_fetch = true;
    options.soft = true;
    let lines = report(&options, ensure(&options).unwrap()).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("map unavailable:"), "{lines:?}");
    let marker = fs::read_to_string(options.packages().join(UNAVAILABLE_MARKER)).unwrap();
    assert_eq!(marker.trim(), lines[0]);
    options.soft = false;
    assert!(report(&options, ensure(&options).unwrap()).is_err());
}

#[test]
fn a_ready_report_gives_hash_directory_and_manifest_and_clears_the_marker() {
    let temp = tempfile::tempdir().unwrap();
    let worker = fake_worker(temp.path(), "exit 1");
    let options = options(temp.path(), worker);
    fs::create_dir_all(options.packages()).unwrap();
    fs::write(
        options.packages().join(UNAVAILABLE_MARKER),
        "map unavailable: old",
    )
    .unwrap();
    let hash = publish_package(&options.packages(), request());
    let lines = report(&options, ensure(&options).unwrap()).unwrap();
    assert_eq!(lines[0], format!("hash={hash}"));
    assert_eq!(lines[1], format!("request={}", cache_key(&request())));
    assert!(lines[2].starts_with("directory=/"), "{lines:?}");
    assert!(lines[3].ends_with(&format!("{hash}.json")), "{lines:?}");
    assert!(!options.packages().join(UNAVAILABLE_MARKER).exists());
}
