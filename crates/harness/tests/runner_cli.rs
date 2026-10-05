use std::{fs, path::Path, process::Command};

#[test]
fn inherited_make_dry_run_touch_and_ignore_errors_cannot_forge_success() {
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Makefile"),
        "alpha:\n\t@printf 'ran recipe\\n'\n\t@false\nbeta:\n\t@true\n",
    )
    .unwrap();
    for flag in ["--just-print", "--touch", "--ignore-errors"] {
        let result = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .current_dir(root.path())
            .env("MAKEFLAGS", flag)
            .env("MFLAGS", flag)
            .env("GNUMAKEFLAGS", flag)
            .args([
                "gate-run",
                "--job",
                "core",
                "--output",
                output.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            !result.status.success(),
            "ambient {flag} must not produce PASS"
        );
        assert_eq!(ledger(output.path())["results"][0]["verdict"], "FAIL");
        assert!(
            fs::read_to_string(output.path().join("alpha.log"))
                .unwrap()
                .contains("ran recipe")
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn cli_sigterm_cancels_owned_make_and_retains_incomplete_ledger() {
    use nix::{
        sys::signal::{Signal, kill},
        unistd::Pid,
    };
    use std::time::{Duration, Instant};
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    // Child PID in external output is only a disposable test witness. It is NOT
    // an authenticated evidence channel (bootstrap-local explicitly says so).
    let marker = output.path().join("owned-pid");
    fs::write(root.path().join("Makefile"), format!("alpha:\n\\t@sleep 30 & child=$$$$!; printf '%s' $$$$child > '{}'; wait $$$$child\nbeta:\n\\t@true\n", marker.display()).replace("\\t", "\t").replace("$$$$", "$$")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .current_dir(root.path())
        .args([
            "gate-run",
            "--job",
            "core",
            "--output",
            output.path().to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !marker.exists() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        marker.exists(),
        "real make recipe must start before cancellation"
    );
    let owned: i32 = fs::read_to_string(&marker).unwrap().parse().unwrap();
    kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM).unwrap();
    let stopped = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        if stopped.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("cancel did not stop gate-run");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ledger(output.path())["overall"], "INCOMPLETE");
    assert_eq!(ledger(output.path())["results"][0]["verdict"], "SKIPPED");
    assert_eq!(
        ledger(output.path())["results"][0]["triage"]["capture"]["outcome"]["kind"],
        "CANCELLED"
    );
    assert!(ledger(output.path())["results"][0]["triage"]["capture"]["duration_ms"].is_number());
    let state = fs::read_to_string(format!("/proc/{owned}/stat")).ok();
    assert!(
        state.as_ref().is_none_or(|state| state
            .split(')')
            .nth(1)
            .is_some_and(|rest| rest.trim_start().starts_with('Z'))),
        "owned child still executing: {state:?}"
    );
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
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
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}
fn fixture() -> tempfile::TempDir {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path();
    git(root, &["init", "--quiet", "--template="]);
    fs::create_dir(root.join("gates")).unwrap();
    fs::write(root.join(".gitignore"), "/.cache/\n/target/\n").unwrap();
    let gate = |id: &str, deps: Vec<&str>| serde_json::json!({"id":id,"command":format!("make {id}"),"requires":deps,"select":"fixture","evidence":"log","suites":["static"],"cadences":["edit","ci"],"budget_s":2,"static":true,"capabilities":[],"blocks":["edit","ci"]});
    let registry = serde_json::json!({"version":2,"suites":[{"id":"everything","paths":["**"],"implies":["static"],"review":true},{"id":"static","paths":["**"],"implies":[],"review":false}],"gates":[gate("alpha",vec![]),gate("beta",vec!["alpha"])],"jobs":{"core":["alpha","beta"]}});
    fs::write(
        root.join("gates/registry.json"),
        serde_json::to_vec_pretty(&registry).unwrap(),
    )
    .unwrap();
    fs::write(root.join("Makefile"), "alpha:\n\t@printf 'successful stdout\\n'\n\t@printf 'successful stderr\\n' >&2\nbeta:\n\t@printf 'dependent\\n'\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "fixture"]);
    owner
}
fn run(root: &Path, output: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .current_dir(root)
        .args([
            "gate-run",
            "--job",
            "core",
            "--output",
            output.to_str().unwrap(),
        ])
        .args(extra)
        .output()
        .unwrap()
}
fn ledger(output: &Path) -> serde_json::Value {
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("ledger.json")).unwrap()).unwrap();
    assert_eq!(
        value["schema"], 2,
        "live triage changes the local ledger contract"
    );
    value
}
fn summary(result: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn real_job_execution_retains_success_logs_and_index_ignores_working_make() {
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    let passing = run(root.path(), output.path(), &[]);
    assert!(
        passing.status.success(),
        "{}",
        String::from_utf8_lossy(&passing.stderr)
    );
    let evidence = ledger(output.path());
    assert_eq!(evidence["overall"], "PASS");
    assert_eq!(
        evidence["results"][0]["triage"]["capture"]["outcome"]["kind"],
        "SUCCESS"
    );
    assert_eq!(summary(&passing)["final_publication"]["kind"], "PUBLISHED");
    assert_eq!(
        summary(&passing)["source_expected"]["revision"],
        evidence["metadata"]["revision"]
    );
    assert_eq!(
        summary(&passing)["endpoints"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["phase"],
        "FINAL_CLI_AFTER_IMAGES"
    );
    assert_eq!(evidence["authoritative"], serde_json::json!(false));
    assert_eq!(evidence["results"].as_array().unwrap().len(), 2);
    let log = fs::read_to_string(output.path().join("alpha.log")).unwrap();
    assert!(log.contains("successful stdout") && log.contains("successful stderr"));
    assert!(
        evidence["canonical_registry_hash"]
            .as_str()
            .unwrap()
            .starts_with("blake3:registry-v2-canonical-v1:")
    );
    fs::write(
        root.path().join("Makefile"),
        "alpha:\n\t@false\nbeta:\n\t@false\n",
    )
    .unwrap();
    let index = run(root.path(), output.path(), &["--scope", "index"]);
    assert!(
        index.status.success(),
        "{}",
        String::from_utf8_lossy(&index.stderr)
    );
    assert_eq!(ledger(output.path())["overall"], "PASS");
    let failed_working = run(root.path(), output.path(), &[]);
    assert!(!failed_working.status.success());
    let failed = ledger(output.path());
    assert_eq!(failed["results"][0]["verdict"], "FAIL");
    assert_eq!(failed["results"][1]["verdict"], "SKIPPED");
    assert_eq!(failed["overall"], "INCOMPLETE");
}

#[test]
fn mutation_empty_budget_and_overlapping_output_do_not_pass() {
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    let zero = run(root.path(), output.path(), &["--total-seconds", "0"]);
    assert!(!zero.status.success());
    assert_eq!(ledger(output.path())["overall"], "INCOMPLETE");
    assert!(
        ledger(output.path())["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|value| value["verdict"] == "SKIPPED")
    );
    assert!(!run(root.path(), root.path(), &[]).status.success());
    fs::write(
        root.path().join("Makefile"),
        "alpha:\n\t@printf '\\n# changed source\\n' >> Makefile\nbeta:\n\t@true\n",
    )
    .unwrap();
    let mutated = run(root.path(), output.path(), &[]);
    assert!(!mutated.status.success());
    assert_eq!(ledger(output.path())["overall"], "INVALID");
    assert_eq!(
        summary(&mutated)["gates"][0]["command"]["observation"]["capture"]["outcome"]["kind"],
        "SUCCESS"
    );
    // The actual snapshot verifier rejects altered export bytes before returning
    // either fingerprint. Do not invent which independent proof was measured.
    assert_eq!(summary(&mutated)["endpoints"][2]["phase"], "POST_GATE");
    assert_eq!(summary(&mutated)["endpoints"][2]["source"], "UNAVAILABLE");
    assert_eq!(summary(&mutated)["endpoints"][2]["private"], "UNAVAILABLE");
}

#[test]
fn numeric_make_failure_secret_tails_and_later_independent_success_reach_safe_console() {
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    let registry_path = root.path().join("gates/registry.json");
    let mut registry: serde_json::Value =
        serde_json::from_slice(&fs::read(&registry_path).unwrap()).unwrap();
    registry["gates"][1]["requires"] = serde_json::json!([]);
    fs::write(registry_path, serde_json::to_vec(&registry).unwrap()).unwrap();
    fs::write(root.path().join("Makefile"), "alpha:\n\t@printf 'STDOUT_SECRET panic: not a diagnosis'; printf 'STDERR_SECRET' >&2; exit 7\nbeta:\n\t@printf 'later success'\n").unwrap();
    let result = run(root.path(), output.path(), &[]);
    assert!(!result.status.success());
    let value = summary(&result);
    let measured = &value["gates"][0]["command"]["observation"]["capture"];
    assert_eq!(measured["outcome"]["kind"], "FAILED");
    assert_eq!(measured["outcome"]["code"], 2);
    assert_eq!(measured["root_cause"], "ROOT_CAUSE_NOT_ASSESSED");
    assert_eq!(
        measured["stdout"]["raw_blake3"],
        blake3::hash(b"STDOUT_SECRET panic: not a diagnosis")
            .to_hex()
            .to_string()
    );
    assert_eq!(
        value["gates"][1]["command"]["observation"]["capture"]["outcome"]["kind"],
        "SUCCESS"
    );
    let console = String::from_utf8_lossy(&result.stdout);
    for forbidden in [
        "STDOUT_SECRET",
        "STDERR_SECRET",
        "diagnosis",
        "reason",
        "path",
        "judge",
    ] {
        assert!(!console.contains(forbidden));
    }
    let raw = fs::read_to_string(output.path().join("alpha.log")).unwrap();
    assert!(raw.contains("STDOUT_SECRET") && raw.contains("STDERR_SECRET"));
}

#[test]
fn live_deadline_preserves_measured_outcome_not_legacy_reason_reconstruction() {
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Makefile"),
        "alpha:\n\t@sleep 5\nbeta:\n\t@true\n",
    )
    .unwrap();
    let result = run(root.path(), output.path(), &["--per-gate-seconds", "1"]);
    assert!(!result.status.success());
    let value = summary(&result);
    let measured = &value["gates"][0]["command"]["observation"]["capture"];
    assert_eq!(measured["outcome"]["kind"], "DEADLINE");
    assert!(measured["duration_ms"].as_u64().unwrap() >= 1000);
    assert_eq!(value["gates"][1]["command"]["kind"], "UNOBSERVED");
}

#[test]
fn measured_success_and_failure_survive_log_and_final_publication_collisions() {
    for failed_command in [false, true] {
        let root = fixture();
        let output = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("Makefile"),
            format!(
                "alpha:\n\t@printf 'RETENTION_SECRET'; {}\nbeta:\n\t@true\n",
                if failed_command { "exit 7" } else { "true" }
            ),
        )
        .unwrap();
        fs::create_dir(output.path().join("alpha.log")).unwrap();
        fs::create_dir(output.path().join("ledger.json")).unwrap();
        let forged = output.path().join("ledger.json/old-pass");
        fs::write(&forged, b"{\"overall\":\"PASS\"}").unwrap();
        let result = run(root.path(), output.path(), &[]);
        assert!(!result.status.success());
        let value = summary(&result);
        let observation = &value["gates"][0]["command"]["observation"];
        assert_eq!(
            observation["capture"]["outcome"]["kind"],
            if failed_command { "FAILED" } else { "SUCCESS" }
        );
        assert_eq!(observation["retention"]["kind"], "FAILED");
        assert_eq!(
            value["gates"][0]["verdict"],
            if failed_command {
                "FAIL"
            } else {
                "UNAVAILABLE"
            }
        );
        assert_eq!(value["final_publication"]["kind"], "FAILED");
        assert_eq!(value["overall"], "INVALID");
        assert!(!String::from_utf8_lossy(&result.stdout).contains("RETENTION_SECRET"));
        assert_eq!(fs::read(forged).unwrap(), b"{\"overall\":\"PASS\"}");
    }
}

#[test]
fn successful_live_commands_still_fail_cli_when_final_publication_fails() {
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    fs::create_dir(output.path().join("ledger.json")).unwrap();
    let forged = output.path().join("ledger.json/forged-pass");
    fs::write(&forged, b"{\"overall\":\"PASS\",\"reason\":\"OLD_SECRET\"}").unwrap();
    let result = run(root.path(), output.path(), &[]);
    assert!(!result.status.success());
    let value = summary(&result);
    assert_eq!(value["execution_overall"], "PASS");
    assert_eq!(value["overall"], "INVALID");
    assert_eq!(value["final_publication"]["kind"], "FAILED");
    assert_eq!(
        value["gates"][0]["command"]["observation"]["capture"]["outcome"]["kind"],
        "SUCCESS"
    );
    assert!(!String::from_utf8_lossy(&result.stdout).contains("OLD_SECRET"));
    assert!(fs::read_to_string(forged).unwrap().contains("OLD_SECRET"));
}

#[cfg(unix)]
#[test]
fn dangling_output_symlink_is_replaced_without_writing_its_missing_target() {
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    let absent = output.path().join("never-written-target");
    std::os::unix::fs::symlink(&absent, output.path().join("ledger.json")).unwrap();
    let result = run(root.path(), output.path(), &[]);
    assert!(result.status.success());
    assert!(!absent.exists());
    assert!(
        !fs::symlink_metadata(output.path().join("ledger.json"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(ledger(output.path())["schema"], 2);
    // Atomic local hygiene only: no promise to erase hostile same-UID old data.
}
