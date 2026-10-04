use super::*;
mod generator;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::TempDir;
fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> String {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["--no-replace-objects", "-c", "core.fsmonitor=false"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0");
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(bytes) = input {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn commit(root: &Path, parent: Option<&str>) -> String {
    git(root, &["add", "."], None);
    let tree = git(root, &["write-tree"], None);
    let parent = parent
        .map(|oid| format!("parent {oid}\n"))
        .unwrap_or_default();
    let raw = format!(
        "tree {tree}\n{parent}author Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\nfixture\n"
    );
    let oid = git(
        root,
        &["hash-object", "-t", "commit", "-w", "--stdin"],
        Some(raw.as_bytes()),
    );
    git(root, &["update-ref", "HEAD", &oid], None);
    oid
}
fn fixture() -> (TempDir, String) {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path();
    git(root, &["init", "-q"], None);
    // Snapshot exports are generated caches, never embedded-repository inputs.
    fs::write(root.join(".gitignore"), ".cache/\n").unwrap();
    fs::create_dir_all(root.join("gates")).unwrap();
    fs::create_dir_all(root.join("crates/demo/src")).unwrap();
    let source = "pub fn bounded(value: u32) -> bool { value < 8 }\n#[cfg(test)] mod tests { #[test] fn accepts() { assert!(crate::bounded(3)); } #[test] fn rejects() { assert!(!crate::bounded(9)); } }\n";
    fs::write(root.join("crates/demo/src/lib.rs"), source).unwrap();
    let real = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::write(
        root.join("gates/registry.json"),
        fs::read(real.join("gates/registry.json")).unwrap(),
    )
    .unwrap();
    let catalog = json!({"schema":1,"requirements":[{"id":"fixture-bounds","risk":"critical","scope":"AST fixture only, semantic execution not assessed.","statement":"Fixture checks bounded values.","invariants":[{"id":"fixture-boundary","claim":"Valid and invalid values exercise the declared boundary.","boundaries":[{"path":"crates/demo/src/lib.rs","symbol":"bounded","kind":"function"}],"case_ids":["fixture-positive","fixture-negative"]}],"gate_refs":["test-unit"]}],"cases":[{"id":"fixture-positive","kind":"positive","path":"crates/demo/src/lib.rs","symbol":"tests::accepts","runner":"native-test","gate":"test-unit"},{"id":"fixture-negative","kind":"negative","path":"crates/demo/src/lib.rs","symbol":"tests::rejects","runner":"native-test","gate":"test-unit"}],"deferred":[]});
    fs::write(
        root.join("gates/contracts.json"),
        serde_json::to_vec(&catalog).unwrap(),
    )
    .unwrap();
    fs::write(root.join("old.txt"), "unchanged bytes").unwrap();
    let oid = commit(root, None);
    (owner, oid)
}
fn snapshots(root: &Path, base: &str, candidate: &str) -> (Snapshot, Snapshot) {
    (
        Snapshot::prepare_independent(root, Kind::Commit(base.into())).unwrap(),
        Snapshot::prepare_independent(root, Kind::Commit(candidate.into())).unwrap(),
    )
}
fn review(subject: &Subject) -> schema::Review {
    schema::Review::parse(&serde_json::to_vec(&json!({"schema":1,"subject":subject,"notes":"Observed exact content; no authentication asserted."})).unwrap()).unwrap()
}
#[test]
fn real_checked_catalog_subject_is_portable_and_never_authorizes_review() {
    let (one, oid) = fixture();
    let (two, other) = fixture();
    assert_eq!(oid, other);
    let (base, candidate) = snapshots(one.path(), &oid, &oid);
    let generated = materialize(&base, &candidate).unwrap();
    let (base2, candidate2) = snapshots(two.path(), &oid, &oid);
    assert_eq!(generated, materialize(&base2, &candidate2).unwrap());
    assert!(generated.affected_paths.is_empty());
    assert_eq!(generated.repository_anchor, "UNAVAILABLE");
    verify_bindings(&generated.contracts, &generated.candidate_content).unwrap();
    let no_review = assessment(&generated, None).unwrap();
    assert_eq!(
        no_review["status"],
        "SUBJECT_MATERIALIZED_NON_AUTHORITATIVE"
    );
    let matched = assessment(&generated, Some(&review(&generated))).unwrap();
    assert_eq!(
        matched["status"],
        "EXACT_SUBJECT_MATCH_REVIEW_NOT_AUTHENTICATED"
    );
    assert_eq!(matched["authoritative"], false);
    assert_eq!(matched["independent_review"], "NOT_ASSESSED");
    assert_eq!(matched["authenticated_verdict"], "UNAVAILABLE");
    fs::write(one.path().join("unrelated.txt"), "new head").unwrap();
    commit(one.path(), Some(&oid));
    let (base3, candidate3) = snapshots(one.path(), &oid, &oid);
    assert_eq!(generated, materialize(&base3, &candidate3).unwrap());
    assert!(recheck(&base, &candidate, &generated).is_err());
}
#[test]
fn actual_index_and_commit_inventories_preserve_both_rename_paths_and_modes() {
    let (owner, base_oid) = fixture();
    let root = owner.path();
    // Copy the same bytes and remove only the resolved owned fixture leaf.
    let old = root.join("old.txt");
    assert_eq!(fs::canonicalize(&old).unwrap(), old);
    fs::copy(&old, root.join("new.txt")).unwrap();
    fs::remove_file(old).unwrap();
    git(root, &["add", "."], None);
    let base = Snapshot::prepare_independent(root, Kind::Commit(base_oid.clone())).unwrap();
    let index = Snapshot::prepare(root, Kind::Index).unwrap();
    let subject = materialize(&base, &index).unwrap();
    assert_eq!(subject.affected_paths, vec!["new.txt", "old.txt"]);
    assert!(matches!(subject.scope, Scope::Index { .. }));
    let candidate_oid = commit(root, Some(&base_oid));
    let (base, candidate) = snapshots(root, &base_oid, &candidate_oid);
    let committed = materialize(&base, &candidate).unwrap();
    assert_eq!(committed.affected_paths, subject.affected_paths);
    assert_ne!(digest(&subject).unwrap(), digest(&committed).unwrap());
    git(root, &["update-index", "--chmod=+x", "new.txt"], None);
    assert!(base.content_witness().is_err());
    let base = Snapshot::prepare_independent(root, Kind::Commit(base_oid)).unwrap();
    let index = Snapshot::prepare(root, Kind::Index).unwrap();
    let modes = materialize(&base, &index).unwrap();
    let before = files(&committed.candidate_content).unwrap();
    let after = files(&modes.candidate_content).unwrap();
    assert_eq!(before["new.txt"]["mode"], "100644");
    assert_eq!(after["new.txt"]["mode"], "100755");
    assert_eq!(
        before["new.txt"]["raw_blake3"],
        after["new.txt"]["raw_blake3"]
    );
}
#[test]
fn every_subject_component_and_nested_claim_is_exact_not_self_reported() {
    let (owner, oid) = fixture();
    let (base, candidate) = snapshots(owner.path(), &oid, &oid);
    let subject = materialize(&base, &candidate).unwrap();
    for pointer in [
        "/schema",
        "/repository_anchor",
        "/scope/base",
        "/scope/candidate",
        "/base_content/digest",
        "/candidate_content/digest",
        "/contracts/digest",
        "/contracts/algorithm",
        "/contracts/bindings/0/binding/line",
        "/canonical_registry_hash",
        "/affected_paths",
        "/required_pr_plan/gates",
        "/mandatory_preflight_plan/gates",
    ] {
        let mut changed = review(&subject);
        *changed.subject.pointer_mut(pointer).unwrap() = json!("forged");
        assert!(assessment(&subject, Some(&changed)).is_err(), "{pointer}");
    }
    let mut unknown = review(&subject);
    unknown.subject["candidate_content"]["approved"] = json!(true);
    assert!(assessment(&subject, Some(&unknown)).is_err());
    let mut mismatched = subject.contracts.clone();
    mismatched["bindings"][0]["binding"]["file_blake3"] = json!("forged");
    assert!(verify_bindings(&mismatched, &subject.candidate_content).is_err());
    let mut outside = subject.contracts.clone();
    outside["bindings"][0]["binding"]["path"] = json!("outside.rs");
    assert!(verify_bindings(&outside, &subject.candidate_content).is_err());
}
#[test]
fn duplicate_keys_at_every_depth_and_unbounded_or_privileged_input_fail_closed() {
    for value in [
        r#"{"schema":1,"schema":1,"subject":{},"notes":"n"}"#,
        r#"{"schema":1,"subject":{"nested":{"a":1,"a":1}},"notes":"n"}"#,
        r#"{"schema":1,"subject":{"a":[{"x":1,"x":1}]},"notes":"n"}"#,
        r#"{"schema":1,"subject":{},"notes":"n","approved":true}"#,
        r#"{"schema":2,"subject":{},"notes":"n"}"#,
    ] {
        assert!(schema::Review::parse(value.as_bytes()).is_err());
    }
    for notes in [
        "".into(),
        " ".into(),
        "newline\nsecret".into(),
        "x".repeat(4097),
    ] {
        assert!(
            schema::Review::parse(
                &serde_json::to_vec(&json!({"schema":1,"subject":{},"notes":notes})).unwrap()
            )
            .is_err()
        );
    }
    let nested = format!(
        "{{\"schema\":1,\"subject\":{}0{},\"notes\":\"n\"}}",
        "[".repeat(40),
        "]".repeat(40)
    );
    assert!(schema::Review::parse(nested.as_bytes()).is_err());
    assert!(schema::Review::parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    let trailing = b"{\"schema\":1,\"subject\":{},\"notes\":\"n\"}{}";
    assert!(schema::Review::parse(trailing).is_err());
}
fn options(base: &str, output: &Path, review: Option<PathBuf>) -> Options {
    Options {
        base: base.into(),
        candidate: Some(base.into()),
        index: false,
        output: output.into(),
        review,
    }
}
#[test]
fn output_aliases_fail_before_pending_and_mismatches_replace_old_acceptance() {
    let (owner, oid) = fixture();
    let output = tempfile::tempdir().unwrap();
    let sentinel = b"old accepted observation";
    fs::write(output.path().join("observation.json"), sentinel).unwrap();
    for input in [
        output.path().to_path_buf(),
        output.path().join("observation.json"),
    ] {
        assert!(execute(owner.path(), options(&oid, output.path(), Some(input))).is_err());
        assert_eq!(
            fs::read(output.path().join("observation.json")).unwrap(),
            sentinel
        );
    }
    let input = tempfile::NamedTempFile::new().unwrap();
    fs::write(
        input.path(),
        b"{\"schema\":1,\"subject\":{},\"notes\":\"untrusted\"}",
    )
    .unwrap();
    assert!(
        execute(
            owner.path(),
            options(&oid, output.path(), Some(input.path().into()))
        )
        .is_err()
    );
    let unavailable: Value =
        serde_json::from_slice(&fs::read(output.path().join("observation.json")).unwrap()).unwrap();
    assert_eq!(unavailable["status"], "UNAVAILABLE");
    assert_eq!(unavailable["authoritative"], false);
    execute(owner.path(), options(&oid, output.path(), None)).unwrap();
    let subject: Value =
        serde_json::from_slice(&fs::read(output.path().join("subject.json")).unwrap()).unwrap();
    fs::write(
        input.path(),
        serde_json::to_vec(&json!({"schema":1,"subject":subject,"notes":"Exact bytes only."}))
            .unwrap(),
    )
    .unwrap();
    execute(
        owner.path(),
        options(&oid, output.path(), Some(input.path().into())),
    )
    .unwrap();
    let observation: Value =
        serde_json::from_slice(&fs::read(output.path().join("observation.json")).unwrap()).unwrap();
    assert_eq!(
        observation["status"],
        "EXACT_SUBJECT_MATCH_REVIEW_NOT_AUTHENTICATED"
    );
    assert_eq!(observation["authenticated_verdict"], "UNAVAILABLE");
}
#[cfg(unix)]
#[test]
fn symlink_alias_is_rejected_before_output_clobber_and_common_git_output_rejected() {
    let (owner, oid) = fixture();
    let output = tempfile::tempdir().unwrap();
    let input = tempfile::tempdir().unwrap();
    let sentinel = b"old observation";
    fs::write(output.path().join("observation.json"), sentinel).unwrap();
    let alias = input.path().join("review");
    std::os::unix::fs::symlink(output.path().join("subject.json"), &alias).unwrap();
    assert!(execute(owner.path(), options(&oid, output.path(), Some(alias))).is_err());
    assert_eq!(
        fs::read(output.path().join("observation.json")).unwrap(),
        sentinel
    );
    assert!(
        execute(
            owner.path(),
            options(&oid, &owner.path().join(".git"), None)
        )
        .is_err()
    );
}
