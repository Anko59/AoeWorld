use std::{path::PathBuf, process::Command};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("checkout root")
}

fn invoke(arguments: &[&str], success: bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .args(arguments)
        .current_dir(root())
        .output()
        .expect("harness command");
    assert_eq!(
        output.status.success(),
        success,
        "{arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn checkout_policies_and_synthetic_protocol_smoke_execute_through_cli() {
    for command in ["structure-check", "architecture-check", "docs-check"] {
        invoke(&[command], true);
    }
    invoke(&["impact", "crates/protocol/src/lib.rs"], true);
    invoke(&["perf-smoke"], true);
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root().join("reports/perf/smoke.json")).expect("report"),
    )
    .expect("JSON report");
    assert_eq!(report["workloads"][0]["total_entities"], 8_000);
    assert_eq!(report["workloads"][0]["clients_connected"], 8);
    assert_eq!(report["workloads"][0]["snapshots"], 8);
    assert_eq!(report["workloads"][0]["deltas"], 16);

    invoke(&["release-verify"], false);
    invoke(&["release-rehearse"], false);
    invoke(&["qa-validate", "missing-report.json"], false);
}

#[test]
fn hooks_install_and_check_in_disposable_git_repository() {
    let directory = tempfile::tempdir().expect("temporary repository");
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(directory.path())
        .output()
        .expect("git init");
    assert!(init.status.success());
    let command = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .args(args)
            .current_dir(directory.path())
            .output()
            .expect("harness hooks command")
    };
    assert!(command(&["hooks-install"]).status.success());
    assert!(command(&["hooks-check"]).status.success());
    std::fs::write(
        directory.path().join(".git/hooks/pre-commit"),
        b"#!/bin/sh\nexit 0\n",
    )
    .expect("tamper hook");
    assert!(!command(&["hooks-check"]).status.success());
}

#[test]
fn ci_selection_manifest_and_aggregate_execute_through_cli() {
    let directory = tempfile::tempdir().expect("temporary output");
    let output_path = directory.path().join("github-output");
    std::fs::write(&output_path, "").expect("output file");
    let select = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .arg("ci-select")
        .env_remove("AOE_BASE_SHA")
        .env("GITHUB_OUTPUT", &output_path)
        .current_dir(root())
        .output()
        .expect("selection");
    assert!(
        select.status.success(),
        "{}",
        String::from_utf8_lossy(&select.stderr)
    );
    let manifest = String::from_utf8(select.stdout).expect("manifest");
    let output = std::fs::read_to_string(output_path).expect("GitHub outputs");
    assert!(output.contains(&format!("manifest={}", manifest.trim())));
    assert!(output.contains("native_coverage=true"));
    let results = serde_json::json!({
        "select":"success", "static":"success", "native-coverage":"success",
        "browser":"success", "target-performance":"success", "fuzz-smoke":"success"
    })
    .to_string();
    let check = |results: &str| {
        Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .arg("ci-check")
            .env_remove("AOE_BASE_SHA")
            .env("AOE_SELECTION_JSON", manifest.trim())
            .env("AOE_JOB_RESULTS_JSON", results)
            .current_dir(root())
            .output()
            .expect("aggregate")
    };
    assert!(check(&results).status.success());
    assert!(
        !check(&results.replace("\"browser\":\"success\"", "\"browser\":\"failure\""))
            .status
            .success()
    );
}
