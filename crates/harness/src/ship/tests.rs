use super::{
    Options,
    evidence::{self, Verdict},
    git, judge, ship,
};
use std::{fs, path::Path, process::Command};

pub(super) fn run(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

fn registry() -> String {
    serde_json::json!({
        "version": 2,
        "suites": [
            {"id": "everything", "paths": ["gates/**"], "implies": ["static"], "review": true},
            {"id": "static", "paths": ["**"], "implies": [], "review": false}
        ],
        "gates": [{
            "id": "fmt-check", "command": "make fmt-check", "requires": [], "select": "fixture",
            "evidence": "exit status", "suites": ["static"], "cadences": ["preflight", "ci"],
            "budget_s": 30, "static": true, "capabilities": [], "blocks": ["preflight"]
        }],
        "jobs": {"static": ["fmt-check"]}
    })
    .to_string()
}

/// A checkout on `feature` whose `origin` is a local bare repository with `dev`.
pub(super) fn fixture(gate: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("temp");
    let origin = temp.path().join("origin.git");
    let root = temp.path().join("work");
    run(
        temp.path(),
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "dev",
            origin.to_str().unwrap(),
        ],
    );
    run(
        temp.path(),
        &["init", "-q", "-b", "dev", root.to_str().unwrap()],
    );
    fs::create_dir_all(root.join("gates")).unwrap();
    fs::write(root.join("gates/registry.json"), registry()).unwrap();
    // The real review config, with floors for this fixture's two suites.
    let mut review: serde_json::Value =
        serde_json::from_str(include_str!("../../../../gates/review.json")).unwrap();
    review["floors"] = serde_json::json!({"static": "low", "everything": "high"});
    fs::write(root.join("gates/review.json"), review.to_string()).unwrap();
    // The registry runs `make <gate>`; the recipe is the fixture's behaviour.
    fs::write(root.join("Makefile"), format!("fmt-check:\n\t@{gate}\n")).unwrap();
    fs::write(root.join("README.md"), "x\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "base"]);
    run(
        &root,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "-b", "feature"]);
    fs::write(root.join("README.md"), "y\n").unwrap();
    run(&root, &["commit", "-q", "-am", "change"]);
    (temp, root)
}

pub(super) fn offline() -> Options {
    Options {
        no_pr: true,
        no_fetch: true,
        no_review: true,
        ..Options::default()
    }
}

#[test]
fn only_a_clean_feature_branch_ships() {
    let (_temp, root) = fixture("true");
    fs::write(root.join("README.md"), "dirty\n").unwrap();
    let error = judge(&root, &offline()).unwrap_err().to_string();
    assert!(error.contains("commit first"), "{error}");
    run(&root, &["checkout", "-q", "README.md"]);
    fs::write(root.join("new.txt"), "untracked\n").unwrap();
    let error = judge(&root, &offline()).unwrap_err().to_string();
    assert!(error.contains("commit first"), "{error}");
    fs::remove_file(root.join("new.txt")).unwrap();
    run(&root, &["checkout", "-q", "dev"]);
    let error = judge(&root, &offline()).unwrap_err().to_string();
    assert!(error.contains("moves only by merging"), "{error}");
    run(&root, &["checkout", "-q", "--detach", "feature"]);
    let error = judge(&root, &offline()).unwrap_err().to_string();
    assert!(error.contains("detached"), "{error}");
    for branch in ["main", "release/1.0"] {
        assert!(git::protected(branch));
    }
    assert!(!git::protected("feature/release"));
}

#[test]
fn a_passing_commit_gets_its_own_evidence_and_is_pushed() {
    let (_temp, root) = fixture("true");
    let evidence = ship(&root, &offline()).expect("ship");
    assert_eq!(evidence.verdict, Verdict::Pass);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(evidence.head, head);
    assert_eq!(evidence.changed, vec!["README.md".to_owned()]);
    let stored = evidence::read(&root, &head).unwrap().expect("stored");
    assert_eq!(stored.gates[0].gate, "fmt-check");
    let remote = git::git(&root, &["ls-remote", "origin", "refs/heads/feature"]).unwrap();
    assert!(remote.starts_with(&head), "{remote}");
    // A new commit has no evidence of its own.
    run(&root, &["commit", "-q", "--allow-empty", "-m", "next"]);
    let next = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(evidence::read(&root, &next).unwrap().is_none());
}

#[test]
fn a_failing_gate_records_evidence_and_pushes_nothing() {
    let (_temp, root) = fixture("echo broken; false");
    let error = ship(&root, &offline()).unwrap_err().to_string();
    assert!(error.contains("FAIL"), "{error}");
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let stored = evidence::read(&root, &head).unwrap().expect("stored");
    assert_eq!(stored.verdict, Verdict::Fail);
    assert!(
        stored.gates[0].tail.contains("broken"),
        "{}",
        stored.gates[0].tail
    );
    let remote = git::git(&root, &["ls-remote", "origin", "refs/heads/feature"]).unwrap();
    assert!(remote.is_empty(), "{remote}");
}

#[test]
fn a_commit_during_the_gates_leaves_no_evidence() {
    let gate = "git -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m sneaky";
    let (_temp, root) = fixture(gate);
    let before = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let error = ship(&root, &offline()).unwrap_err().to_string();
    assert!(error.contains("changed while the gates ran"), "{error}");
    assert!(evidence::read(&root, &before).unwrap().is_none());
}

#[test]
fn a_local_branch_named_origin_dev_does_not_move_the_base() {
    let (_temp, root) = fixture("true");
    run(&root, &["branch", "origin/dev", "feature"]);
    let remote = git::git(&root, &["rev-parse", "refs/remotes/origin/dev"]).unwrap();
    let (base, _) = git::base(&root, "dev", false).unwrap();
    assert_eq!(base, remote);
}

#[test]
fn the_low_tier_size_check_fetches_the_current_origin_dev() {
    let (_temp, root) = fixture("true");
    let old = git::git(&root, &["rev-parse", "refs/remotes/origin/dev"]).unwrap();
    run(&root, &["checkout", "-q", "dev"]);
    std::fs::write(root.join("README.md"), "new remote base\n").unwrap();
    run(&root, &["commit", "-q", "-am", "remote advancement"]);
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["update-ref", "refs/remotes/origin/dev", &old]);
    let stale = git::git(&root, &["rev-parse", "refs/remotes/origin/dev"]).unwrap();
    let current = git::git(&root, &["ls-remote", "origin", "refs/heads/dev"])
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    assert_ne!(stale, current);
    super::github::changed_lines(&root).unwrap();
    assert_eq!(
        git::git(&root, &["rev-parse", "refs/remotes/origin/dev"]).unwrap(),
        current
    );
}

#[test]
fn incomplete_gates_never_count_as_a_pass() {
    use super::run::{GateResult, GateVerdict};
    let result = |verdict| GateResult {
        gate: "g".into(),
        verdict,
        seconds: 0.0,
        summary: String::new(),
        tail: String::new(),
    };
    assert_eq!(evidence::verdict(&[]), Verdict::Incomplete);
    assert_eq!(
        evidence::verdict(&[result(GateVerdict::Pass), result(GateVerdict::Unavailable)]),
        Verdict::Incomplete
    );
    assert_eq!(
        evidence::verdict(&[result(GateVerdict::Unavailable), result(GateVerdict::Fail)]),
        Verdict::Fail
    );
    assert_eq!(
        evidence::verdict(&[result(GateVerdict::Pass)]),
        Verdict::Pass
    );
}

#[test]
fn gh_versions_below_the_attach_release_are_refused() {
    assert_eq!(super::github::GH_MINIMUM, (2, 100));
    assert!(super::github::gh_version("gh version 2.99.0").unwrap() < super::github::GH_MINIMUM);
    assert!(super::github::gh_version("gh version 2.100.0").unwrap() >= super::github::GH_MINIMUM);
    assert_eq!(
        super::github::gh_version("gh version 2.102.0 (2026-09-30)"),
        Some((2, 102))
    );
    assert!(
        super::github::gh_version("gh version 2.46.0 (2025-12-13 Ubuntu 2.46.0-4)").unwrap()
            < super::github::GH_MINIMUM
    );
    assert_eq!(super::github::gh_version("nonsense"), None);
}

#[test]
fn the_branch_and_repository_come_from_full_refs_and_origin() {
    let (_temp, root) = fixture("true");
    run(&root, &["tag", "feature"]);
    assert_eq!(git::branch(&root).unwrap(), "feature");
    for (url, expected) in [
        (
            "git@github.com:Anko59/AoeWorld.git",
            Some("Anko59/AoeWorld"),
        ),
        (
            "https://github.com/Anko59/AoeWorld",
            Some("Anko59/AoeWorld"),
        ),
        (
            "ssh://git@github.com/Anko59/AoeWorld.git",
            Some("Anko59/AoeWorld"),
        ),
        ("https://gitlab.com/x/y.git", None),
        ("git@github.com:onlyowner", None),
    ] {
        run(&root, &["remote", "set-url", "origin", url]);
        assert_eq!(
            git::origin_repository(&root).ok().as_deref(),
            expected,
            "{url}"
        );
    }
}

mod describe;
mod metrics;
mod showcase;
