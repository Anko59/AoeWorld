use super::super::{marked_body, parse_issue_pages, test_stub::Stub};
use super::*;
use crate::ship::metrics::snapshot;
use std::{fs, process::Command};

fn limits() -> Thresholds {
    Thresholds::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
}

fn metrics(tests: u64, unsafe_count: u64, todo: u64, deps: u64, over: &[&str]) -> Metrics {
    let mut metrics = Metrics {
        unsafe_count,
        todo_count: todo,
        files_over_limit: over.iter().map(|p| (*p).to_owned()).collect(),
        ..Metrics::default()
    };
    metrics.tests.insert("unit".into(), tests);
    metrics.dependencies.insert("cargo".into(), deps);
    metrics
}

fn owned_issue(number: u64) -> Issue {
    let body = marked_body("old", TITLE).unwrap();
    parse_issue_pages(
        serde_json::json!([{
            "number": number, "title": TITLE, "body": body, "user": {"login": "github-actions[bot]"},
            "labels": [{"name": "nightly-failure"}], "html_url": "u"
        }])
        .to_string()
        .as_bytes(),
    )
    .unwrap()
    .remove(0)
}

#[test]
fn the_committed_thresholds_load_and_bad_ones_are_refused() {
    let limits = limits();
    assert_eq!((limits.window_hours, limits.max_test_drop), (24, 0));
    let temp = tempfile::tempdir().unwrap();
    assert!(Thresholds::load(temp.path()).is_err());
    fs::create_dir(temp.path().join("gates")).unwrap();
    for text in [
        r#"{"version": 2}"#,
        r#"{"version":1,"window_hours":0,"max_test_drop":0,"max_unsafe_increase":0,"max_todo_increase":0,"max_dependency_increase":0,"max_files_over_limit":0}"#,
        r#"{"version":1,"window_hours":1,"max_test_drop":0,"max_unsafe_increase":0,"max_todo_increase":0,"max_dependency_increase":0,"max_files_over_limit":0,"extra":1}"#,
    ] {
        fs::write(temp.path().join(CONFIG), text).unwrap();
        assert!(Thresholds::load(temp.path()).is_err(), "{text}");
    }
}

#[test]
fn each_regression_beyond_its_threshold_is_listed() {
    let limits = limits();
    let old = metrics(100, 2, 10, 50, &[]);
    assert!(regressions(Some(&old), &metrics(100, 2, 15, 55, &[]), &limits).is_empty());
    assert!(regressions(Some(&old), &metrics(120, 1, 3, 40, &[]), &limits).is_empty());
    let found = regressions(
        Some(&old),
        &metrics(99, 3, 16, 56, &["a.rs", "b.md"]),
        &limits,
    );
    assert_eq!(found.len(), 5, "{found:?}");
    assert!(found[0].contains("fell from 100 to 99"));
    assert!(found[1].contains("`unsafe` lines rose from 2 to 3"));
    assert!(found[2].contains("TODO/FIXME lines rose from 10 to 16"));
    assert!(found[3].contains("dependencies rose from 50 to 56"));
    assert!(found[4].contains("2 files exceed the 500-line limit: `a.rs`, `b.md`"));
    // Without an earlier commit only the absolute file limit applies.
    let many: Vec<String> = (0..25).map(|n| format!("f{n}.md")).collect();
    let refs: Vec<&str> = many.iter().map(String::as_str).collect();
    let alone = regressions(None, &metrics(0, 9, 99, 99, &refs), &limits);
    assert_eq!(alone.len(), 1);
    assert!(
        alone[0].starts_with("25 files") && alone[0].ends_with(", …"),
        "{alone:?}"
    );
}

#[test]
fn one_owned_issue_is_opened_updated_or_closed() {
    let findings = vec!["Test count fell".to_owned()];
    let opened = plan(&findings, &[], "RUN");
    assert!(matches!(&opened[..], [triage::Action::Open { title, body }]
        if title == TITLE && body.contains("- Test count fell") && body.contains("RUN")));
    let issues = [owned_issue(4), owned_issue(5)];
    let updated = plan(&findings, &issues, "RUN");
    assert!(
        matches!(&updated[..], [triage::Action::Update { number: 4, .. }]),
        "{updated:?}"
    );
    assert!(matches!(
        &plan(&[], &issues, "RUN")[..],
        [
            triage::Action::Close { number: 4, .. },
            triage::Action::Close { number: 5, .. }
        ]
    ));
    let forged = parse_issue_pages(
        serde_json::json!([{
            "number": 8, "title": TITLE, "body": marked_body("x", TITLE).unwrap(),
            "user": {"login": "mallory"}, "labels": [{"name": "nightly-failure"}], "html_url": "u"
        }])
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    assert!(plan(&[], &forged, "RUN").is_empty());
    assert!(matches!(
        &plan(&findings, &forged, "RUN")[..],
        [triage::Action::Open { .. }]
    ));
}

fn commit(root: &Path, date: Option<&str>, message: &str) {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(["commit", "-q", "--allow-empty", "-m", message]);
    if let Some(date) = date {
        command
            .env("GIT_COMMITTER_DATE", date)
            .env("GIT_AUTHOR_DATE", date);
    }
    assert!(command.status().unwrap().success());
}

#[test]
fn the_baseline_is_the_newest_commit_before_the_window() {
    let stub = Stub::new(&serde_json::json!([]));
    assert_eq!(baseline(&stub.root, 24).unwrap(), None);
    commit(&stub.root, Some("2020-01-01T00:00:00 +0000"), "old");
    let old = crate::ship::git::git(&stub.root, &["rev-parse", "HEAD"]).unwrap();
    commit(
        &stub.root,
        Some("2020-01-02T00:00:00 +0000"),
        "older but recent enough",
    );
    commit(&stub.root, None, "new");
    assert_eq!(
        baseline(&stub.root, 24).unwrap().as_deref(),
        Some(
            crate::ship::git::git(&stub.root, &["rev-parse", "HEAD~1"])
                .unwrap()
                .as_str()
        )
    );
    assert_ne!(
        baseline(&stub.root, 24).unwrap().as_deref(),
        Some(old.as_str())
    );
    assert_eq!(baseline(&stub.root, 24 * 365 * 10).unwrap(), None);
}

fn repository_with(stub: &Stub, tests: usize, long: usize) {
    fs::create_dir_all(stub.root.join("gates")).unwrap();
    let config = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(CONFIG);
    fs::copy(config, stub.root.join(CONFIG)).unwrap();
    fs::create_dir_all(stub.root.join("crates/x/src")).unwrap();
    fs::write(
        stub.root.join("crates/x/src/tests.rs"),
        "#[test]\nfn t() {}\n".repeat(tests),
    )
    .unwrap();
    fs::write(stub.root.join("docs.md"), "line\n".repeat(long)).unwrap();
}

fn git(stub: &Stub, args: &[&str]) {
    assert!(
        Command::new("git")
            .current_dir(&stub.root)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn the_nightly_run_opens_updates_and_closes_the_issue_on_origin() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set("GITHUB_RUN_ID", "77");
    stub.set("GIT_AUTHOR_DATE", "2020-01-01T00:00:00 +0000");
    stub.set("GIT_COMMITTER_DATE", "2020-01-01T00:00:00 +0000");
    repository_with(&stub, 3, 10);
    git(&stub, &["add", "-A"]);
    git(
        &stub,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e.x",
            "commit",
            "-q",
            "-m",
            "before",
        ],
    );
    stub.unset("GIT_AUTHOR_DATE");
    stub.unset("GIT_COMMITTER_DATE");
    repository_with(&stub, 1, 501);
    git(&stub, &["add", "-A"]);
    git(
        &stub,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e.x",
            "commit",
            "-q",
            "-m",
            "after",
        ],
    );

    execute(&stub.root).unwrap();
    let calls = stub.calls();
    assert!(
        calls.contains("<--title>\n<Nightly: metric regressions>"),
        "{calls}"
    );
    let bodies = stub.bodies();
    assert!(bodies.contains("Test count fell from 3 to 1"), "{bodies}");
    assert!(bodies.contains("`docs.md`"), "{bodies}");
    assert!(bodies.contains("actions/runs/77"), "{bodies}");
    assert!(bodies.contains("aoe-fingerprint"));

    // The next run edits the same issue.
    let listing = serde_json::json!([{
        "number": 12, "title": TITLE, "body": marked_body("old", TITLE).unwrap(),
        "user": {"login": "github-actions[bot]"}, "labels": [{"name": "nightly-failure"}], "html_url": "u"
    }]);
    stub.set("GH_LIST_JSON", &listing.to_string());
    stub.clear();
    execute(&stub.root).unwrap();
    assert!(
        stub.calls()
            .contains("<issue>\n<edit>\n<12>\n<--body-file>\n<->"),
        "{}",
        stub.calls()
    );

    // Healthy again: the issue is closed.
    repository_with(&stub, 9, 10);
    git(&stub, &["add", "-A"]);
    git(
        &stub,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e.x",
            "commit",
            "-q",
            "-m",
            "fixed",
        ],
    );
    stub.clear();
    execute(&stub.root).unwrap();
    assert!(
        stub.calls().contains("<issue>\n<close>\n<12>"),
        "{}",
        stub.calls()
    );
    stub.set("AOE_AGENT_ROLE", "tester");
    assert!(
        execute(&stub.root)
            .unwrap_err()
            .to_string()
            .contains("only to the main session")
    );
}

#[test]
fn the_route_make_target_and_workflow_reach_the_regression_check() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: &str| fs::read_to_string(root.join(path)).unwrap();
    let harness = read(".agents/hooks/harness.sh");
    assert!(
        harness.contains("metric-regressions) probe=crates/harness/src/ship/issues/regressions.rs")
    );
    assert!(harness.contains("metrics) probe=crates/harness/src/ship/metrics/snapshot.rs"));
    let make = read("make/ship.mk");
    assert!(
        make.contains("metric-regressions:\n\t@.agents/hooks/harness.sh exec metric-regressions")
    );
    assert!(make.contains("metrics:\n\t@.agents/hooks/harness.sh exec metrics"));
    assert!(make.contains(" METRICS_REV"));
    let workflow = read(".github/workflows/nightly.yml");
    let (_, triage) = workflow.split_once("\n  triage:\n").unwrap();
    assert!(triage.contains("fetch-depth: 0"));
    assert!(triage.contains("run: make metric-regressions"));
    let _ = snapshot::path;
}
