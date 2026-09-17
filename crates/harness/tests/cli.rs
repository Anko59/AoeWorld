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
