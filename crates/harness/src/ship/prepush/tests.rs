use super::reusable;
use crate::ship::{
    evidence::{self, Evidence, Verdict},
    recheck::Recheck,
    run::{GateResult, GateVerdict},
};
use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q"]);
    git(root.path(), &["config", "user.email", "t@example.com"]);
    git(root.path(), &["config", "user.name", "t"]);
    std::fs::write(root.path().join("a"), "a\n").unwrap();
    git(root.path(), &["add", "a"]);
    git(root.path(), &["commit", "-q", "-m", "a"]);
    root
}

fn record(root: &Path, verdict: Verdict, cadence: &str) -> String {
    let head = git(root, &["rev-parse", "HEAD"]);
    evidence::write(
        root,
        &Evidence {
            version: 1,
            cadence: cadence.into(),
            head: head.clone(),
            tree: git(root, &["rev-parse", "HEAD^{tree}"]),
            branch: "feature".into(),
            base: "dev".into(),
            base_branch: "dev".into(),
            merge_base: head.clone(),
            changed: Vec::new(),
            gates: Vec::new(),
            verdict,
            level: None,
            recheck: None,
            started: 0,
            finished: 0,
        },
    )
    .unwrap();
    head
}

#[test]
fn passing_preflight_evidence_for_the_clean_pushed_head_is_reused() {
    let root = repository();
    let head = record(root.path(), Verdict::Pass, "preflight");
    assert!(reusable(root.path(), Some(&head)).unwrap());
}

#[test]
fn anything_else_runs_the_preflight() {
    let root = repository();
    let head = git(root.path(), &["rev-parse", "HEAD"]);
    // No claim, a malformed claim, or no evidence at all.
    assert!(!reusable(root.path(), None).unwrap());
    assert!(!reusable(root.path(), Some("HEAD")).unwrap());
    assert!(!reusable(root.path(), Some(&head)).unwrap());
    // Failing, incomplete or other-cadence evidence.
    for (verdict, cadence) in [
        (Verdict::Fail, "preflight"),
        (Verdict::Incomplete, "preflight"),
        (Verdict::Pass, "pr"),
    ] {
        record(root.path(), verdict, cadence);
        assert!(
            !reusable(root.path(), Some(&head)).unwrap(),
            "{verdict:?} {cadence}"
        );
    }
    // Passing evidence, but the claim is not HEAD or the tree is dirty.
    record(root.path(), Verdict::Pass, "preflight");
    std::fs::write(root.path().join("a"), "changed\n").unwrap();
    assert!(!reusable(root.path(), Some(&head)).unwrap());
    git(root.path(), &["commit", "-qam", "b"]);
    assert!(!reusable(root.path(), Some(&head)).unwrap());
}

fn gate(verdict: GateVerdict) -> GateResult {
    GateResult {
        gate: "fmt-check".into(),
        verdict,
        seconds: 1.0,
        summary: String::new(),
        tail: String::new(),
    }
}

fn evidence_of(
    root: &Path,
    head: &str,
    base: &str,
    cadence: &str,
    gates: Vec<GateResult>,
) -> Evidence {
    Evidence {
        version: 1,
        cadence: cadence.into(),
        head: head.into(),
        tree: git(root, &["rev-parse", &format!("{head}^{{tree}}")]),
        branch: "feature".into(),
        base: base.into(),
        base_branch: "dev".into(),
        merge_base: base.into(),
        changed: Vec::new(),
        verdict: evidence::verdict(&gates),
        gates,
        level: None,
        recheck: None,
        started: 0,
        finished: 0,
    }
}

/// A branch commit whose full preflight passed, then rebased onto a moved
/// base by the re-check: `(preflight head, rebased head, rebased evidence)`.
fn rebased(root: &Path) -> (String, String, Evidence) {
    let old_base = git(root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("own"), "own\n").unwrap();
    git(root, &["add", "own"]);
    git(root, &["commit", "-q", "-m", "own"]);
    let from = git(root, &["rev-parse", "HEAD"]);
    let preflight = evidence_of(
        root,
        &from,
        &old_base,
        "preflight",
        vec![gate(GateVerdict::Pass)],
    );
    evidence::write(root, &preflight).unwrap();
    git(root, &["checkout", "-q", "-b", "moved", &old_base]);
    std::fs::write(root.join("theirs"), "theirs\n").unwrap();
    git(root, &["add", "theirs"]);
    git(root, &["commit", "-q", "-m", "theirs"]);
    let new_base = git(root, &["rev-parse", "HEAD"]);
    git(root, &["cherry-pick", &from]);
    let head = git(root, &["rev-parse", "HEAD"]);
    let mut after = evidence_of(
        root,
        &head,
        &new_base,
        "recheck",
        vec![gate(GateVerdict::Pass)],
    );
    after.recheck = Some(Recheck {
        head: head.clone(),
        base: new_base,
        from_head: from.clone(),
        from_base: old_base,
        preflight: preflight.gates,
        judge: None,
        rounds: 1,
        commits: Vec::new(),
    });
    (from, head, after)
}

#[test]
fn a_rebased_head_is_accepted_through_its_recheck_record() {
    let root = repository();
    let (_, head, after) = rebased(root.path());
    assert!(
        !reusable(root.path(), Some(&head)).unwrap(),
        "no record written yet"
    );
    evidence::write(root.path(), &after).unwrap();
    assert!(reusable(root.path(), Some(&head)).unwrap());
}

#[test]
fn a_rebased_head_is_refused_without_a_valid_recheck_record() {
    let root = repository();
    let (from, head, valid) = rebased(root.path());
    let refused = |evidence: &Evidence, why: &str| {
        evidence::write(root.path(), evidence).unwrap();
        assert!(!reusable(root.path(), Some(&head)).unwrap(), "{why}");
    };
    let mut copied = valid.clone();
    copied.cadence = "preflight".into();
    refused(
        &copied,
        "a copied preflight verdict with a record is not the record",
    );
    let mut failed = valid.clone();
    failed.gates = vec![gate(GateVerdict::Fail)];
    failed.verdict = Verdict::Fail;
    refused(&failed, "the static gates failed");
    let mut other_head = valid.clone();
    other_head.recheck.as_mut().unwrap().head = from.clone();
    refused(&other_head, "the record names another head");
    let mut other_from = valid.clone();
    other_from.recheck.as_mut().unwrap().from_head = head.clone();
    refused(&other_from, "the record names itself as the preflight head");
    let mut no_preflight = valid.clone();
    no_preflight.recheck.as_mut().unwrap().from_head = "0".repeat(40);
    refused(&no_preflight, "no preflight evidence for the named head");
    // The preflight failed on the named head.
    let old_base = valid.recheck.as_ref().unwrap().from_base.clone();
    let mut bad = evidence_of(
        root.path(),
        &from,
        &old_base,
        "preflight",
        vec![gate(GateVerdict::Fail)],
    );
    evidence::write(root.path(), &bad).unwrap();
    refused(&valid, "the full preflight did not pass on the named head");
    bad.verdict = Verdict::Pass;
    bad.gates = vec![gate(GateVerdict::Pass)];
    evidence::write(root.path(), &bad).unwrap();
    evidence::write(root.path(), &valid).unwrap();
    assert!(reusable(root.path(), Some(&head)).unwrap(), "restored");
}

#[test]
fn a_rebased_head_with_another_own_patch_is_refused() {
    let root = repository();
    let (_, _, valid) = rebased(root.path());
    // The rebased commit gained a line the preflighted patch did not have.
    std::fs::write(root.path().join("own"), "own\nmore\n").unwrap();
    git(root.path(), &["commit", "-qam", "more"]);
    let head = git(root.path(), &["rev-parse", "HEAD"]);
    let mut changed = evidence_of(
        root.path(),
        &head,
        &valid.base,
        "recheck",
        vec![gate(GateVerdict::Pass)],
    );
    let mut record = valid.recheck.clone().unwrap();
    record.head = head.clone();
    changed.recheck = Some(record);
    evidence::write(root.path(), &changed).unwrap();
    assert!(!reusable(root.path(), Some(&head)).unwrap());
}
