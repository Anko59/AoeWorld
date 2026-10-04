//! CLI observations are model-only, never authenticated deployment qualification.
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
fn cli(root: &Path, input: &Path, output: &Path) -> std::process::Output {
    cli_options(root, input, output, &[])
}
fn cli_options(root: &Path, input: &Path, output: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .current_dir(root)
        .args([
            "supervisor-model",
            "--requirements",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .args(extra)
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .output()
        .unwrap()
}
fn git_init(root: &Path) {
    let result = Command::new("/usr/bin/git")
        .current_dir(root)
        .args(["init", "--quiet", "--template="])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
#[cfg(unix)]
fn durable_model_journal_reuses_history_and_corruption_never_reuses_ready_feedback() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    git_init(root.path());
    let owner = tempfile::tempdir().unwrap();
    let output = owner.path().join("output");
    fs::create_dir(&output).unwrap();
    fs::set_permissions(&output, fs::Permissions::from_mode(0o700)).unwrap();
    let input = owner.path().join("requirements.json");
    fs::write(
        &input,
        serde_json::to_vec(&requirements(owner.path())).unwrap(),
    )
    .unwrap();
    for expected in [2, 4] {
        let result = cli_options(
            root.path(),
            &input,
            &output,
            &["--persist-model-journal", "--probe-service"],
        );
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: Value =
            serde_json::from_slice(&fs::read(output.join("supervisor-model.json")).unwrap())
                .unwrap();
        assert_eq!(report["status"], "UNAVAILABLE");
        assert_eq!(report["authoritative"], false);
        assert_eq!(report["local_journal"]["sequence"], expected);
        assert_eq!(report["local_journal"]["authoritative"], false);
        assert_eq!(report["local_journal"]["fsync_calls_completed"], true);
        assert_eq!(report["daemon_probe"]["authoritative"], false);
    }
    let journal = output.join("lease-journal/journal.json");
    fs::write(&journal, b"invalid history").unwrap();
    fs::write(
        output.join("supervisor-model.json"),
        br#"{"status":"READY","authoritative":true}"#,
    )
    .unwrap();
    assert!(
        !cli_options(root.path(), &input, &output, &["--persist-model-journal"])
            .status
            .success()
    );
    let report: Value =
        serde_json::from_slice(&fs::read(output.join("supervisor-model.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "UNAVAILABLE");
    assert_eq!(report["authoritative"], false);
    assert_eq!(fs::read(journal).unwrap(), b"invalid history");
}
fn requirements(owner: &Path) -> Value {
    for name in ["artifact", "service-evidence", "leases"] {
        fs::create_dir(owner.join(name)).unwrap();
    }
    let dispatch: Vec<_> = [
        "fmt-check",
        "structure-check",
        "architecture-check",
        "docs-check",
        "lint",
        "test-unit",
    ]
    .iter()
    .map(|name| json!({"gate":name,"operation":name,"image":"rust-tools"}))
    .collect();
    json!({"schema":1,"anchor":{"schema":1,"repository":"Example/Policy","repository_id":7,"remote_url":"https://github.com/Example/Policy.git","integration_branch":"dev"},"service_uid":71231,"candidate_uid":65532,"artifact_root":owner.join("artifact"),"evidence_root":owner.join("service-evidence"),"lease_root":owner.join("leases"),"subject":{"schema":1,"repository_id":7,"protected_ref":"refs/heads/dev","protected_commit":"1".repeat(40),"protected_tree":"2".repeat(40),"closure_blake3":"3".repeat(64),"registry_hash":format!("blake3:registry-v2-canonical-v1:{}","4".repeat(64)),"executable_blake3":"5".repeat(64),"runtime_blake3":"6".repeat(64),"trust_root_id":"7".repeat(64),"abi":{"schema":1,"abi":1,"registry_schema":2,"images":[{"name":"rust-tools","reference":format!("ghcr.io/anko59/aoeworld/rust-tools@sha256:{}","8".repeat(64)),"actual_id":format!("sha256:{}","9".repeat(64))}],"dispatch":dispatch}},"resources":{"memory_mib":1024,"pids":128,"cpus":2,"workload_s":30,"cleanup_s":15,"command_s":5}})
}
#[test]
fn well_formed_model_retains_unavailable_admission_and_invalid_input_replaces_stale_ready() {
    let root = tempfile::tempdir().unwrap();
    git_init(root.path());
    let owner = tempfile::tempdir().unwrap();
    let output = owner.path().join("output");
    fs::create_dir(&output).unwrap();
    let input = owner.path().join("requirements.json");
    let value = requirements(owner.path());
    fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    let bytes = fs::read(&input).unwrap();
    let result = cli(root.path(), &input, &output);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(output.join("supervisor-model.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "UNAVAILABLE");
    assert_eq!(report["authoritative"], false);
    assert_eq!(
        report["requirements_blake3"],
        blake3::hash(&bytes).to_hex().to_string()
    );
    assert_eq!(report["artifact_signature"]["status"], "ABSENT");
    assert_eq!(report["artifact_signature"]["authoritative"], false);
    assert!(report.get("lease_models").is_some());
    assert_eq!(fs::read(&input).unwrap(), bytes);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(output.join("supervisor-model.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let mut signed_claim = value.clone();
    signed_claim["attestation"] =
        json!({"schema":1,"key_id":"7".repeat(64),"signature":"not-a-signature"});
    fs::write(&input, serde_json::to_vec(&signed_claim).unwrap()).unwrap();
    assert!(cli(root.path(), &input, &output).status.success());
    let report: Value =
        serde_json::from_slice(&fs::read(output.join("supervisor-model.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "UNAVAILABLE");
    assert_eq!(report["authoritative"], false);
    assert_eq!(report["artifact_signature"]["status"], "REJECTED");
    signed_claim["attestation"]["public_key"] = json!("candidate-chosen-key");
    fs::write(&input, serde_json::to_vec(&signed_claim).unwrap()).unwrap();
    assert!(!cli(root.path(), &input, &output).status.success());
    for bad in [
        b"not-json".to_vec(),
        vec![b' '; 32769],
        serde_json::to_vec(&json!({"verified":true,"cid":"a".repeat(64)})).unwrap(),
    ] {
        fs::write(
            output.join("supervisor-model.json"),
            br#"{"authoritative":true,"status":"READY"}"#,
        )
        .unwrap();
        fs::write(&input, bad).unwrap();
        let result = cli(root.path(), &input, &output);
        assert!(!result.status.success());
        let report: Value =
            serde_json::from_slice(&fs::read(output.join("supervisor-model.json")).unwrap())
                .unwrap();
        assert_eq!(report["authoritative"], false);
        assert_eq!(report["status"], "UNAVAILABLE");
        assert!(report["reason"].is_string());
    }
}
#[cfg(unix)]
#[test]
fn input_aliases_fail_without_clobbering_and_linked_requirements_never_admit() {
    let root = tempfile::tempdir().unwrap();
    git_init(root.path());
    let owner = tempfile::tempdir().unwrap();
    let output = owner.path().join("output");
    fs::create_dir(&output).unwrap();
    let alias = owner.path().join("alias");
    std::os::unix::fs::symlink(&output, &alias).unwrap();
    let reserved = output.join("supervisor-model.json");
    let original = b"caller-owned input";
    fs::write(&reserved, original).unwrap();
    assert!(
        !cli(root.path(), &alias.join("supervisor-model.json"), &output)
            .status
            .success()
    );
    assert_eq!(fs::read(&reserved).unwrap(), original);
    let missing = owner.path().join("never-created.json");
    let dangling = output.join("dangling-requirements.json");
    std::os::unix::fs::symlink(&missing, &dangling).unwrap();
    assert!(!cli(root.path(), &dangling, &output).status.success());
    assert!(!missing.exists());
    assert_eq!(fs::read(&reserved).unwrap(), original);
    let real = owner.path().join("real.json");
    fs::write(&real, b"{}").unwrap();
    let link = owner.path().join("input.json");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(!cli(root.path(), &link, &output).status.success());
    let report: Value = serde_json::from_slice(&fs::read(&reserved).unwrap()).unwrap();
    assert_eq!(report["authoritative"], false);
    let hard = owner.path().join("hard.json");
    fs::hard_link(&real, &hard).unwrap();
    assert!(!cli(root.path(), &hard, &output).status.success());
    assert_eq!(fs::read(&real).unwrap(), b"{}");
}
