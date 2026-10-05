use super::*;
use serde_json::{Value, json};
use std::fs;
mod io;
mod outcomes;

pub(super) fn git(root: &Path, args: &[&str]) -> String {
    let mut command = std::process::Command::new("git");
    command.current_dir(root);
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    let output = command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
pub(super) fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q"]);
    git(root.path(), &["config", "user.name", "fixture"]);
    git(
        root.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    fs::write(root.path().join("source.rs"), b"original committed bytes\n").unwrap();
    fs::write(root.path().join(".gitignore"), b".cache/\nreports/\n").unwrap();
    git(root.path(), &["add", "source.rs", ".gitignore"]);
    git(root.path(), &["commit", "-qm", "fixture"]);
    root
}

fn sample() -> Outcomes {
    Outcomes {
        cargo_mutants_version: TOOL_VERSION.into(),
        total_mutants: 42,
        caught: 38,
        missed: 0,
        timeout: 0,
        unviable: 4,
        success: 0,
        start_time: Some("started".into()),
        end_time: Some("completed".into()),
        outcomes: Vec::new(),
    }
}
fn phase(name: &str, status: Value) -> Value {
    json!({"phase":name,"duration":0.001,"process_status":status,"argv":["cargo","test"]})
}
fn descriptor(index: usize) -> Value {
    json!({"name":format!("{}:{index}:1: synthetic mutation",FILES[0]),"package":"aoe-harness",
        "file":FILES[0],"function":{"function_name":"compare","return_type":"",
        "span":{"start":{"line":1,"column":1},"end":{"line":100,"column":2}}},
        "span":{"start":{"line":index+1,"column":1},"end":{"line":index+1,"column":2}},
        "replacement":"","genre":"BinaryOperator"})
}
pub(super) fn artifacts(caught: usize, unviable: usize) -> (Value, Value) {
    let mut inventory = Vec::new();
    let mut records = vec![
        json!({"scenario":"Baseline","summary":"Success","log_path":"log/baseline.log",
        "diff_path":null,"phase_results":[phase("Build",json!("Success")),phase("Test",json!("Success"))]}),
    ];
    for index in 0..caught + unviable {
        let mutant = descriptor(index);
        let mut entry = mutant.clone();
        entry["diff"] = json!("--- source\n+++ synthetic mutation\n");
        inventory.push(entry);
        let (summary, phases) = if index < caught {
            (
                "CaughtMutant",
                vec![
                    phase("Build", json!("Success")),
                    phase("Test", json!({"Failure":101})),
                ],
            )
        } else {
            ("Unviable", vec![phase("Build", json!({"Failure":101}))])
        };
        records.push(json!({"scenario":{"Mutant":mutant},"summary":summary,
            "log_path":format!("log/mutant-{index}.log"),"diff_path":format!("diff/mutant-{index}.diff"),
            "phase_results":phases}));
    }
    (
        json!({"cargo_mutants_version":TOOL_VERSION,"total_mutants":caught+unviable,
        "caught":caught,"missed":0,"timeout":0,"unviable":unviable,"success":0,
        "start_time":"started","end_time":"completed","outcomes":records}),
        json!(inventory),
    )
}
pub(super) fn install(root: &Path, wire: &Value, inventory: &Value) {
    let output = root.join(OUTPUT).join("mutants.out");
    fs::create_dir_all(&output).unwrap();
    fs::write(
        output.join("outcomes.json"),
        serde_json::to_vec(wire).unwrap(),
    )
    .unwrap();
    fs::write(
        output.join("mutants.json"),
        serde_json::to_vec(inventory).unwrap(),
    )
    .unwrap();
}
fn parse(wire: &Value, inventory: &Value) -> Result<Outcomes> {
    super::outcomes::parse(
        &serde_json::to_vec(wire).unwrap(),
        &serde_json::to_vec(inventory).unwrap(),
    )
}

#[test]
fn options_full_oids_and_exclusive_intentional_scope_fail_closed() {
    use clap::Parser;
    #[derive(clap::Parser)]
    struct Cli {
        #[command(flatten)]
        options: Options,
    }
    assert!(Cli::try_parse_from(["fixture", "--revision", "HEAD"]).is_err());
    assert!(Cli::try_parse_from(["fixture", "--revision", &"A".repeat(40)]).is_err());
    assert!(
        Cli::try_parse_from([
            "fixture",
            "--revision",
            &"a".repeat(40),
            "--intentional-index"
        ])
        .is_err()
    );
    assert!(Cli::try_parse_from(["fixture", "--revision", &"a".repeat(64)]).is_ok());
    assert!(
        Cli::try_parse_from(["fixture", "--intentional-index"])
            .unwrap()
            .options
            .intentional_index
    );
}
#[test]
fn intentional_index_is_captured_once_and_old_commit_is_allowed() {
    let root = repository();
    let head = scopes::resolved_head(root.path()).unwrap();
    fs::write(root.path().join("source.rs"), b"pending index\n").unwrap();
    git(root.path(), &["add", "source.rs"]);
    fs::write(
        root.path().join("source.rs"),
        b"different unstaged working bytes\n",
    )
    .unwrap();
    let index = prepare(
        root.path(),
        &Options {
            revision: None,
            intentional_index: true,
        },
    )
    .unwrap();
    assert_eq!(
        fs::read(index.root().join("source.rs")).unwrap(),
        b"pending index\n"
    );
    let committed = prepare(root.path(), &Options::default()).unwrap();
    assert_eq!(
        fs::read(committed.root().join("source.rs")).unwrap(),
        b"original committed bytes\n"
    );
    git(root.path(), &["add", "source.rs"]);
    assert!(index.verify_source().is_err());
    let fresh = prepare(
        root.path(),
        &Options {
            revision: None,
            intentional_index: true,
        },
    )
    .unwrap();
    assert_eq!(
        fs::read(fresh.root().join("source.rs")).unwrap(),
        b"different unstaged working bytes\n"
    );
    git(root.path(), &["commit", "-qm", "later"]);
    let older = prepare(
        root.path(),
        &Options {
            revision: Some(head),
            intentional_index: false,
        },
    )
    .unwrap();
    assert_eq!(
        fs::read(older.root().join("source.rs")).unwrap(),
        b"original committed bytes\n"
    );
}
#[test]
fn default_commit_ignores_poisoned_git_environment_in_isolated_process() {
    const MARKER: &str = "AOE_MUTATION_SCOPE_ENV_CHILD";
    if std::env::var_os(MARKER).is_some() {
        let root = Path::new(&std::env::var(MARKER).unwrap()).to_path_buf();
        let snapshot = prepare(&root, &Options::default()).unwrap();
        assert_eq!(
            fs::read(snapshot.root().join("source.rs")).unwrap(),
            b"original committed bytes\n"
        );
        assert!(
            scopes::git_directories(&root)
                .unwrap()
                .iter()
                .all(|path| path.starts_with(&root))
        );
        return;
    }
    let root = repository();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mutation::tests::default_commit_ignores_poisoned_git_environment_in_isolated_process",
            "--nocapture",
        ])
        .env(MARKER, root.path())
        .env("GIT_DIR", "/not-a-repository")
        .env("GIT_WORK_TREE", "/not-a-worktree")
        .env("GIT_INDEX_FILE", "/poison-index")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.fsmonitor")
        .env("GIT_CONFIG_VALUE_0", "poison-command")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"));
}
#[test]
fn scanner_actually_receives_every_reported_file() {
    let args = execution::scanner_args(Path::new("/tmp/fresh-mutation-output")).unwrap();
    let files: Vec<_> = args
        .windows(2)
        .filter(|pair| pair[0] == "--file")
        .map(|pair| pair[1].as_str())
        .collect();
    assert_eq!(files, FILES);
    assert!(files.contains(&"crates/harness/src/gates/registry/mod.rs"));
    assert_eq!(
        &args[..6],
        &[
            "mutants",
            "--in-place",
            "--timeout",
            "120",
            "--output",
            "/tmp/fresh-mutation-output"
        ]
    );
    assert_eq!(&args[6..10], &["--re", FILTER, "--exclude-re", EXCLUDE]);
    assert_eq!(args[10], "--cargo-arg=--locked");
}
#[test]
fn incomplete_or_uncaught_mutations_cannot_pass() {
    assert_eq!(assess(&sample(), true).0, Verdict::Pass);
    assert_eq!(assess(&sample(), false).0, Verdict::Inconclusive);
    let mut sample = sample();
    sample.missed = 1;
    sample.caught -= 1;
    assert_eq!(assess(&sample, false).0, Verdict::Regression);
    sample.missed = 0;
    sample.timeout = 1;
    assert_eq!(assess(&sample, false).0, Verdict::Regression);
    sample.end_time = None;
    assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
    sample.end_time = Some("complete".into());
    sample.total_mutants = 1;
    assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
    sample.total_mutants = 42;
    sample.cargo_mutants_version = "wrong".into();
    assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
}
#[test]
fn missing_outcomes_yield_inconclusive_report() {
    let temp = tempfile::tempdir().unwrap();
    let command: Result<()> = Err("missing tool".into());
    let report = write_report(temp.path(), &command).unwrap();
    assert_eq!(report.verdict, Verdict::Inconclusive);
    assert_eq!(
        report.artifact_failure,
        Some(ArtifactFailure::InputUnavailable)
    );
    assert_eq!(report.command_observation.status, "FAILED_UNCLASSIFIED");
    assert!(temp.path().join("reports/mutation/nightly.json").is_file());
    assert!(temp.path().join("reports/mutation/nightly.md").is_file());
}
#[test]
fn complete_synthetic_fixture_outcomes_write_non_authoritative_pass_and_regression_reports() {
    let temp = tempfile::tempdir().unwrap();
    let (mut wire, inventory) = artifacts(38, 4);
    install(temp.path(), &wire, &inventory);
    let report = write_report(temp.path(), &Ok(())).unwrap();
    assert_eq!(report.verdict, Verdict::Pass);
    assert_eq!(report.total_mutants, Some(42));
    assert_eq!(report.evaluated_mutants, Some(38));
    // The unit-only structural adapter has NO retained subject; never read
    // unrelated caller Git CWD to manufacture a revision or source binding.
    assert!(report.revision.is_none());
    assert!(report.mutant_set_hash.is_some());
    assert_eq!(report.source_identity, "UNAVAILABLE");
    assert!(!report.authoritative);
    // Counter/phase reconciliation is structural fixture data, not execution proof.
    wire["outcomes"][1]["summary"] = json!("MissedMutant");
    wire["outcomes"][1]["phase_results"][1]["process_status"] = json!("Success");
    wire["missed"] = json!(1);
    wire["caught"] = json!(37);
    install(temp.path(), &wire, &inventory);
    let report = write_report(temp.path(), &Err("mutants failed".into())).unwrap();
    assert_eq!(report.verdict, Verdict::Regression);
}
#[test]
fn typed_process_failure_is_separate_from_artifact_failure_and_never_passes_old_claims() {
    let temp = tempfile::tempdir().unwrap();
    let (wire, inventory) = artifacts(30, 0);
    install(temp.path(), &wire, &inventory);
    let command: Result<()> = Err(process::ProcessError::Exit {
        program: "secret-program".into(),
        code: Some(7),
        log: "secret-path".into(),
    }
    .into());
    let report = write_report(temp.path(), &command).unwrap();
    assert_eq!(report.verdict, Verdict::Inconclusive);
    assert_eq!(report.command_observation.status, "FAILED");
    assert_eq!(report.command_observation.exit_code, Some(7));
    let safe = serde_json::to_string(&report.command_observation).unwrap();
    assert!(!safe.contains("secret"));
    assert!(report.command_failure.as_ref().unwrap().contains("secret"));
    fs::write(
        temp.path().join(OUTPUT).join("mutants.out/outcomes.json"),
        b"{}",
    )
    .unwrap();
    let report = write_report(temp.path(), &command).unwrap();
    assert_eq!(
        report.artifact_failure,
        Some(ArtifactFailure::SchemaInvalid)
    );
    assert_eq!(report.command_observation.exit_code, Some(7));
    assert_eq!(report.root_cause, "ROOT_CAUSE_NOT_ASSESSED");
    for command in [
        process::ProcessError::Start {
            program: "secret".into(),
            source: std::io::Error::other("secret"),
        },
        process::ProcessError::Deadline {
            program: "secret".into(),
            seconds: 4500,
            log: "secret".into(),
        },
        process::ProcessError::Cancelled {
            program: "secret".into(),
            log: "secret".into(),
        },
        process::ProcessError::Monitor {
            program: "secret".into(),
            source: std::io::Error::other("secret"),
        },
    ] {
        let command: Result<()> = Err(command.into());
        let observed = CommandObservation::observe(&command);
        assert!(!serde_json::to_string(&observed).unwrap().contains("secret"));
        assert_ne!(observed.status, "SUCCESS");
    }
}
#[test]
fn malformed_artifacts_invalidate_previous_local_pass_both_publications() {
    let temp = tempfile::tempdir().unwrap();
    let (wire, inventory) = artifacts(30, 0);
    install(temp.path(), &wire, &inventory);
    assert_eq!(
        write_report(temp.path(), &Ok(())).unwrap().verdict,
        Verdict::Pass
    );
    fs::write(
        temp.path().join(OUTPUT).join("mutants.out/mutants.json"),
        b"[]",
    )
    .unwrap();
    let report = write_report(temp.path(), &Ok(())).unwrap();
    assert_eq!(report.verdict, Verdict::Inconclusive);
    let machine: Value = serde_json::from_slice(
        &fs::read(temp.path().join("reports/mutation/nightly.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(machine["verdict"], "INCONCLUSIVE");
    assert_eq!(machine["artifact_freshness"], "UNAVAILABLE");
    assert_eq!(machine["execution_binding"], "UNAVAILABLE");
    assert_eq!(machine["source_identity"], "UNAVAILABLE");
    assert!(
        !fs::read_to_string(temp.path().join("reports/mutation/nightly.md"))
            .unwrap()
            .contains("Pass")
    );
}
