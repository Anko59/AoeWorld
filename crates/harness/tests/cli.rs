mod ci;

use std::{path::PathBuf, process::Command};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("checkout root")
}

#[test]
fn disk_floor_refuses_real_commands_before_any_slot_or_ship_work() {
    // A slot attempt must fail immediately, even if the parent test holds a slot.
    // A regular file cannot be a queue directory; refusal must precede touching it.
    let unusable_queue = tempfile::NamedTempFile::new().unwrap();
    for arguments in [
        vec!["disk-precheck"],
        vec!["ship"],
        vec!["test-unit"],
        vec!["test-wasm"],
        vec!["test-e2e"],
        vec!["preflight"],
        vec!["slot", "test-unit", "--", "false"],
        vec!["slot", "test-wasm", "--", "false"],
        vec!["slot", "test-e2e", "--", "false"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .args(&arguments)
            .current_dir(root())
            .env_remove("CI")
            .env_remove("GITHUB_ACTIONS")
            .env_remove("AOE_HEAVY_SLOT")
            .env("AOE_HEAVY_DIR", unusable_queue.path())
            .env("AOE_DISK_FLOOR_GIB", "1000000")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{arguments:?}");
        assert!(
            stderr.contains("disk pre-check:"),
            "{arguments:?}: {stderr}"
        );
        assert!(stderr.contains("1000000 GiB"), "{stderr}");
        assert!(!stderr.contains("heavy slot:"), "{stderr}");
    }
}

#[test]
fn disk_precheck_cli_can_be_disabled_and_skips_both_ci_markers() {
    for (name, value) in [
        ("AOE_DISK_FLOOR_GIB", "0"),
        ("CI", "true"),
        ("GITHUB_ACTIONS", "true"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .arg("disk-precheck")
            .current_dir(root())
            .env_remove("CI")
            .env_remove("GITHUB_ACTIONS")
            .env("AOE_DISK_FLOOR_GIB", "1000000")
            .env(name, value)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn make_checks_disk_before_any_requested_gate_takes_a_slot() {
    for target in ["test-unit", "test-wasm", "test-e2e", "preflight"] {
        let output = Command::new("make")
            .args(["--dry-run", target])
            .current_dir(root())
            .env_remove("MAKEFLAGS")
            .env_remove("MFLAGS")
            .env_remove("GNUMAKEFLAGS")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{target}: {output:?}");
        let check = text
            .find("/debug/aoe-harness' disk-precheck")
            .expect("host pre-check");
        let slot = text.find("aoe-harness slot ").expect("gate slot");
        assert!(check < slot, "{target}: {text}");
        assert!(text.contains("-e AOE_DISK_FLOOR_GIB"), "{text}");
        assert!(text.contains("-e CI"), "{text}");
    }
}

fn invoke(arguments: &[&str], success: bool) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aoe-harness"));
    command.args(arguments).current_dir(root());
    let _slot = ci::admission::inherit_or_fixture(&mut command);
    let output = command.output().expect("harness command");
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

fn hook_fixture_command(program: &str) -> Command {
    let mut command = Command::new(program);
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    command
}

#[test]
fn hooks_install_and_check_in_disposable_git_repository() {
    let directory = tempfile::tempdir().expect("temporary repository");
    let init = hook_fixture_command("git")
        .args(["init", "-q"])
        .current_dir(directory.path())
        .output()
        .expect("git init");
    assert!(init.status.success());
    let command = |args: &[&str]| {
        hook_fixture_command(env!("CARGO_BIN_EXE_aoe-harness"))
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
        "browser":"success", "e2e":"success", "target-performance":"success", "fuzz-smoke":"success",
        "qualification":"success", "exhaustive":"success"
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
    assert!(
        !check(&results.replace("\"e2e\":\"success\"", "\"e2e\":\"failure\""))
            .status
            .success()
    );
}
mod wasm_cli;

#[test]
fn the_host_disk_check_runs_the_binary_from_the_selected_target_cache() {
    // Reviewers' trigger: an external HARNESS_TARGET_CACHE builds the harness there, so the
    // host check must run that binary, not the checkout's target/debug/aoe-harness.
    let output = Command::new("make")
        .args([
            "--dry-run",
            "disk-precheck",
            "HARNESS_TARGET_CACHE=/mnt/cache/aoe-target",
        ])
        .current_dir(root())
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .env_remove("GNUMAKEFLAGS")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    let line = text
        .lines()
        .find(|line| line.contains("disk-precheck") && line.contains("aoe-harness"))
        .expect("the host check line");
    assert!(
        line.contains("/mnt/cache/aoe-target/debug/aoe-harness"),
        "{line}"
    );
}
