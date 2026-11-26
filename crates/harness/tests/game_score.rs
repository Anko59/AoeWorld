//! `aoe-harness game-score` end to end: real child processes at different
//! worker counts play a small match on a stored map package, and the runs must
//! agree on every hash and every order.
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest, Ratio};
use serde_json::Value;
use std::{path::Path, process::Command};

/// A small synthetic package in `directory`; its hash.
fn store_package(directory: &Path) -> String {
    let request = MapRequest {
        requested_side_meters: 1_000,
        compression: Ratio::new(1, 1).expect("ratio"),
        ..MapRequest::default()
    };
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("package");
    let hash = package.content_hash_hex();
    std::fs::write(
        directory.join(format!("{hash}.json")),
        serde_json::to_vec(&package).expect("package json"),
    )
    .expect("package written");
    hash
}

/// Runs the game score on a fresh package in a fresh directory.
fn game_score(arguments: &[&str]) -> (std::process::Output, Value, tempfile::TempDir) {
    let directory = tempfile::tempdir().expect("directory");
    let packages = directory.path().join("packages");
    std::fs::create_dir(&packages).expect("package directory");
    store_package(&packages);
    let out = directory.path().join("score.json");
    let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .arg("game-score")
        .args([
            "--package-directory",
            packages.to_str().expect("utf-8 path"),
        ])
        .args(["--seats", "3", "--ticks", "100", "--hash-every", "5"])
        .args([
            "--sample-every",
            "25",
            "--out",
            out.to_str().expect("utf-8 path"),
        ])
        .args(arguments)
        .current_dir(directory.path())
        .output()
        .expect("the harness runs");
    let report = std::fs::read_to_string(&out)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    (output, report, directory)
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn final_hash(run: &Value) -> &Value {
    let digests = run["digests"].as_array().expect("digests");
    &digests.last().expect("a final digest")["hash"]
}

#[test]
fn game_score_one_worker_and_three_workers_play_the_same_game() {
    let (output, report, _directory) = game_score(&["--workers", "1,3"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let runs = report["runs"].as_array().expect("runs");
    assert_eq!(runs.len(), 2);
    assert_eq!(report["determinism"]["holds"], true, "{report}");
    assert_eq!(final_hash(&runs[0]), final_hash(&runs[1]));
    assert_eq!(runs[0]["order_log_hash"], runs[1]["order_log_hash"]);
    assert!(
        runs[0]["order_count"].as_u64().is_some_and(|n| n > 0),
        "no orders"
    );
    assert_eq!(runs[0]["counters"]["units"], runs[1]["counters"]["units"]);
    assert!(runs[0]["counters"]["units"].as_u64().is_some_and(|n| n > 0));
    assert!(
        runs[0]["ticks_per_second"]
            .as_f64()
            .is_some_and(|n| n > 0.0)
    );
    assert!(runs[0]["peak_memory_mib"].as_u64().is_some_and(|n| n > 0));
    let table = report["table"].as_str().expect("the table");
    assert!(
        table.contains("1 worker") && table.contains("3 workers"),
        "{table}"
    );
    assert!(stderr(&output).contains("units alive at the end"));
}

#[test]
fn game_score_two_runs_at_one_worker_count_agree() {
    let (output, report, _directory) = game_score(&["--workers", "2,2"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(report["determinism"]["holds"], true, "{report}");
}

#[test]
fn game_score_without_orders_still_compares_hashes() {
    let (output, report, _directory) = game_score(&["--workers", "1,2", "--no-orders"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(report["runs"][0]["order_count"], Value::Null);
    assert_eq!(report["determinism"]["holds"], true, "{report}");
}

#[test]
fn game_score_names_the_missing_package() {
    let directory = tempfile::tempdir().expect("directory");
    let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .args(["game-score", "--package-directory"])
        .arg(directory.path())
        .current_dir(directory.path())
        .output()
        .expect("the harness runs");
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no map package"),
        "{}",
        stderr(&output)
    );
}
