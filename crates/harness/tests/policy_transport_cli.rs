//! OFFLINE transport simulation, not GitHub authentication or OCI qualification.
//! Copies to crates/harness/tests/policy_transport_cli.rs after parent review.
#![cfg(unix)]
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn real_git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("/usr/bin/git")
        .current_dir(root)
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}
struct Fixture {
    owner: tempfile::TempDir,
    candidate: PathBuf,
    evidence: PathBuf,
    anchor: PathBuf,
    tools: PathBuf,
    source_oid: String,
    candidate_oid: String,
    before: Vec<Vec<u8>>,
}
impl Fixture {
    fn new(abi: &str, mode: &str) -> Self {
        let owner = tempfile::tempdir().unwrap();
        let candidate = owner.path().join("candidate");
        let policy = owner.path().join("policy");
        let tools = owner.path().join("tools");
        let evidence = owner.path().join("evidence");
        for directory in [&candidate, &policy, &tools, &evidence] {
            fs::create_dir(directory).unwrap();
        }
        for root in [&candidate, &policy] {
            real_git(root, &["init", "--quiet", "--template="]);
        }
        fs::write(
            candidate.join("candidate.txt"),
            b"never execute candidate recipes\n",
        )
        .unwrap();
        fs::write(
            candidate.join("Makefile"),
            b"all:\n\t@touch SHOULD_NOT_EXIST\n",
        )
        .unwrap();
        real_git(&candidate, &["add", "."]);
        real_git(&candidate, &["commit", "--quiet", "-m", "candidate"]);
        let candidate_oid = real_git(&candidate, &["rev-parse", "HEAD"]);
        fs::create_dir(policy.join("gates")).unwrap();
        let registry = json!({"version":2,"suites":[{"id":"everything","paths":["**"],"implies":["static"],"review":true},{"id":"static","paths":["**"],"implies":[],"review":false}],"gates":[{"id":"fmt-check","command":"make fmt-check","requires":[],"select":"fixture","evidence":"log","suites":["static"],"cadences":["pr","ci"],"budget_s":2,"static":true,"capabilities":[],"blocks":["pr"]}],"jobs":{"static":["fmt-check"]}});
        fs::write(
            policy.join("gates/registry.json"),
            serde_json::to_vec(&registry).unwrap(),
        )
        .unwrap();
        let reference = format!(
            "ghcr.io/anko59/aoeworld/rust-tools@sha256:{}",
            "1".repeat(64)
        );
        let actual_id = format!("sha256:{}", "2".repeat(64));
        let mut descriptor = json!({"schema":1,"abi":1,"registry_schema":2,"images":[{"name":"rust-tools","reference":reference,"actual_id":actual_id}],"dispatch":[{"gate":"fmt-check","operation":"fmt-check","image":"rust-tools"}]});
        match abi {
            "legacy" => (),
            "malformed" => {
                fs::write(policy.join("gates/judge.json"), b"not-json\n").unwrap();
            }
            other => {
                if other == "no-dispatch" {
                    descriptor["dispatch"] = json!([]);
                }
                if other == "unsupported" {
                    descriptor["abi"] = json!(2);
                }
                fs::write(
                    policy.join("gates/judge.json"),
                    serde_json::to_vec(&descriptor).unwrap(),
                )
                .unwrap();
            }
        }
        real_git(&policy, &["add", "."]);
        real_git(&policy, &["commit", "--quiet", "-m", "protected fixture"]);
        let source_oid = real_git(&policy, &["rev-parse", "HEAD"]);
        let bundle = owner.path().join("protected.bundle");
        real_git(
            &policy,
            &["bundle", "create", bundle.to_str().unwrap(), "HEAD"],
        );
        let anchor = owner.path().join("anchor.json");
        fs::write(&anchor, serde_json::to_vec(&json!({"schema":1,"repository":"Example/Policy","repository_id":7,"remote_url":"https://github.com/Example/Policy.git","integration_branch":"dev"})).unwrap()).unwrap();
        let alias = owner.path().join("anchor-copy.json");
        fs::copy(&anchor, &alias).unwrap();
        let config = owner.path().join("transport.json");
        fs::write(&config, serde_json::to_vec(&json!({"mode":mode,"oid":source_oid,"bundle":bundle,"reference":reference,"actual_id":actual_id,"anchor":anchor,"anchor_copy":alias,"evidence":evidence,"transcript":owner.path().join("transcript.jsonl"),"started":owner.path().join("started")})).unwrap()).unwrap();
        let script = include_str!("policy_transport/transport.py").replace(
            "CONFIG_PATH_LITERAL",
            &serde_json::to_string(config.to_str().unwrap()).unwrap(),
        );
        for name in ["git", "gh", "docker"] {
            let file = tools.join(name);
            fs::write(&file, &script).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let before = ["HEAD", "index", "config"]
            .iter()
            .map(|name| fs::read(candidate.join(".git").join(name)).unwrap())
            .collect();
        Self {
            owner,
            candidate,
            evidence,
            anchor,
            tools,
            source_oid,
            candidate_oid,
            before,
        }
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aoe-harness"));
        command
            .current_dir(&self.candidate)
            .env("PATH", format!("{}:/usr/bin:/bin", self.tools.display()))
            .env("GH_TOKEN", "fixture-only-not-a-real-credential")
            .env("GITHUB_TOKEN", "fixture-only-not-a-real-credential")
            .env("GIT_INDEX_FILE", self.candidate.join(".git/index"))
            .env(
                "GIT_CONFIG_GLOBAL",
                self.owner.path().join("hostile-missing-config"),
            )
            .args([
                "policy-prepare",
                "--anchor",
                self.anchor.to_str().unwrap(),
                "--candidate",
                &self.candidate_oid,
                "--cadence",
                "pr",
                "--output",
                self.evidence.to_str().unwrap(),
            ]);
        command
    }
    fn run(&self) -> Output {
        fs::write(
            self.evidence.join("preparation.json"),
            b"{\"status\":\"PREPARED_NON_AUTHORITATIVE\",\"authoritative\":true}",
        )
        .unwrap();
        self.command().output().unwrap()
    }
    fn descriptor(&self) -> Value {
        serde_json::from_slice(&fs::read(self.evidence.join("preparation.json")).unwrap()).unwrap()
    }
    fn requests(&self) -> Vec<Value> {
        fs::read_to_string(self.owner.path().join("transcript.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    fn verify_candidate(&self) {
        assert_eq!(
            real_git(&self.candidate, &["rev-parse", "HEAD"]),
            self.candidate_oid
        );
        for (index, name) in ["HEAD", "index", "config"].iter().enumerate() {
            assert_eq!(
                fs::read(self.candidate.join(".git").join(name)).unwrap(),
                self.before[index]
            );
        }
        assert!(!self.candidate.join("SHOULD_NOT_EXIST").exists());
        assert!(
            !fs::read_dir(&self.evidence).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("policy-fetch-")),
            "owned fetch repository must drop after invocation"
        );
        for entry in fs::read_dir(&self.evidence).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                assert_eq!(
                    fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
    }
}
#[test]
fn simulated_transport_runs_complete_fixed_oid_resolver_and_prepares_only_non_authoritative_plan() {
    let fixture = Fixture::new("supported", "good");
    let result = fixture.run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let descriptor = fixture.descriptor();
    let preparation = &descriptor["preparation"];
    assert_eq!(preparation["status"], "PREPARED_NON_AUTHORITATIVE");
    assert_eq!(preparation["authoritative"], false);
    assert_eq!(preparation["candidate_commit"], fixture.candidate_oid);
    assert_eq!(preparation["source"]["commit"], fixture.source_oid);
    assert_eq!(preparation["gates"], json!(["fmt-check"]));
    assert!(preparation["canonical_registry_hash"].as_str().is_some());
    let closure: Value =
        serde_json::from_slice(&fs::read(fixture.evidence.join("closure.json")).unwrap()).unwrap();
    assert_eq!(closure["source"]["commit"], fixture.source_oid);
    assert_eq!(closure["entries"].as_array().unwrap().len(), 2);
    assert_eq!(closure["blake3"], preparation["closure_blake3"]);
    let requests = fixture.requests();
    assert_eq!(
        requests
            .iter()
            .filter(|value| value["tool"] == "gh")
            .count(),
        3
    );
    assert_eq!(
        requests
            .iter()
            .filter(|value| value["network"] == true)
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|value| value["tool"] == "docker")
            .count(),
        1
    );
    for name in [
        "policy-api-repository.log",
        "policy-api-branch.log",
        "policy-api-protection.log",
        "policy-ls-remote.log",
        "policy-fetch.log",
        "policy-object-type.log",
        "policy-tree.log",
        "policy-detach.log",
        "policy-index.log",
        "image-rust-tools.log",
    ] {
        assert!(fixture.evidence.join(name).is_file(), "missing {name}");
    }
    fixture.verify_candidate();
}
#[test]
fn moving_or_invalid_remote_observations_fail_before_fetch_and_replace_stale_ready() {
    for (mode, reason) in [
        ("moving", "moved"),
        ("repository-mismatch", "identity"),
        ("checks-malformed", "missing string context"),
        ("api-json", "expected"),
        ("api-exit", "failed"),
        ("api-truncated", "truncated"),
    ] {
        let fixture = Fixture::new("supported", mode);
        let result = fixture.run();
        assert!(!result.status.success(), "{mode}");
        let descriptor = fixture.descriptor();
        assert_eq!(descriptor["status"], "UNAVAILABLE");
        assert_eq!(descriptor["authoritative"], false);
        assert!(
            descriptor["reasons"].to_string().contains(reason),
            "{mode}: {descriptor}"
        );
        assert!(
            !fixture
                .requests()
                .iter()
                .any(|value| value["fetch"] == true),
            "{mode} must reject before fetch"
        );
        assert!(!fixture.evidence.join("closure.json").exists());
        fixture.verify_candidate();
    }
}
#[test]
fn malformed_fetched_objects_or_abi_never_fall_back_to_candidate_policy() {
    for (abi, mode, reason) in [
        ("supported", "object-type", "not a commit"),
        ("supported", "bad-tree", "identifier"),
        ("malformed", "good", "expected"),
        ("unsupported", "good", "unsupported"),
    ] {
        let fixture = Fixture::new(abi, mode);
        assert!(!fixture.run().status.success());
        let descriptor = fixture.descriptor();
        assert_eq!(descriptor["status"], "UNAVAILABLE");
        assert_eq!(descriptor["authoritative"], false);
        assert!(
            descriptor["reasons"].to_string().contains(reason),
            "{abi}/{mode}: {descriptor}"
        );
        fixture.verify_candidate();
    }
}
#[test]
fn legacy_missing_dispatch_and_image_observation_failures_keep_complete_nonpassing_descriptors() {
    for (abi, mode, reason) in [
        ("legacy", "good", "migration"),
        ("no-dispatch", "good", "dispatch"),
        ("supported", "image-id", "config ID"),
        ("supported", "image-exit", "config ID"),
    ] {
        let fixture = Fixture::new(abi, mode);
        assert!(!fixture.run().status.success());
        let descriptor = fixture.descriptor();
        let prepared = &descriptor["preparation"];
        assert_eq!(prepared["status"], "UNAVAILABLE");
        assert_eq!(prepared["authoritative"], false);
        assert!(
            prepared["reasons"].to_string().contains(reason),
            "{abi}/{mode}: {descriptor}"
        );
        if abi != "legacy" {
            assert_eq!(prepared["gates"], json!(["fmt-check"]));
        }
        fixture.verify_candidate();
    }
}
#[test]
fn anchor_endpoint_alias_and_export_shadow_invalidate_ready_preparation_with_retained_error() {
    for mode in [
        "anchor-bytes",
        "anchor-symlink",
        "policy-shadow",
        "image-utf8",
    ] {
        let fixture = Fixture::new("supported", mode);
        assert!(!fixture.run().status.success());
        let descriptor = fixture.descriptor();
        assert_eq!(descriptor["status"], "UNAVAILABLE", "{mode}: {descriptor}");
        assert_eq!(descriptor["authoritative"], false);
        assert!(descriptor["reasons"].as_array().unwrap().len() == 1);
        fixture.verify_candidate();
    }
}
#[test]
fn sigterm_during_resolver_retains_cancelled_receipt_and_never_ready_descriptor() {
    let fixture = Fixture::new("supported", "cancel");
    let mut child = fixture
        .command()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let marker = fixture.owner.path().join("started");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "resolver exited before cancellation marker"
        );
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("fixture resolver did not start");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let result = child.wait_with_output().unwrap();
    assert!(!result.status.success());
    let descriptor = fixture.descriptor();
    assert_eq!(descriptor["status"], "UNAVAILABLE");
    assert_eq!(descriptor["authoritative"], false);
    assert!(descriptor["reasons"].to_string().contains("cancelled"));
    assert!(
        String::from_utf8(fs::read(fixture.evidence.join("policy-api-branch.log")).unwrap())
            .unwrap()
            .contains("fixture waiting")
    );
    fixture.verify_candidate();
}
