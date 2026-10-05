use super::*;
use std::{
    io::Write,
    process::{Command, Stdio},
};
use tempfile::TempDir;
pub(super) fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> String {
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
pub(super) fn commit(root: &Path, parent: Option<&str>) -> String {
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
pub(super) fn fixture() -> (TempDir, String, String) {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path();
    git(root, &["init", "-q"], None);
    fs::write(root.join(".gitignore"), ".cache/\nreports/\n").unwrap();
    fs::create_dir_all(root.join("gates")).unwrap();
    fs::create_dir_all(root.join("crates/demo/src")).unwrap();
    fs::write(root.join("crates/demo/src/lib.rs"), "pub fn bounded(value:u32)->bool {value<8}\n#[cfg(test)] mod tests {#[test] fn accepts(){assert!(crate::bounded(3));} #[test] fn rejects(){assert!(!crate::bounded(9));}}\n").unwrap();
    fs::write(
        root.join("gates/registry.json"),
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../gates/registry.json")).unwrap(),
    )
    .unwrap();
    let catalog = json!({"schema":1,"requirements":[{"id":"fixture-bounds","risk":"critical","scope":"AST fixture only, semantic execution not assessed.","statement":"Fixture checks bounded values.","invariants":[{"id":"fixture-boundary","claim":"Declared boundary.","boundaries":[{"path":"crates/demo/src/lib.rs","symbol":"bounded","kind":"function"}],"case_ids":["fixture-positive","fixture-negative"]}],"gate_refs":["test-unit"]}],"cases":[{"id":"fixture-positive","kind":"positive","path":"crates/demo/src/lib.rs","symbol":"tests::accepts","runner":"native-test","gate":"test-unit"},{"id":"fixture-negative","kind":"negative","path":"crates/demo/src/lib.rs","symbol":"tests::rejects","runner":"native-test","gate":"test-unit"}],"deferred":[]});
    fs::write(
        root.join("gates/contracts.json"),
        serde_json::to_vec(&catalog).unwrap(),
    )
    .unwrap();
    fs::write(root.join("old.txt"), "baseline").unwrap();
    let base = commit(root, None);
    fs::write(root.join("old.txt"), "candidate").unwrap();
    let candidate = commit(root, Some(&base));
    (owner, base, candidate)
}
pub(super) fn snapshots(root: &Path, base: &str, candidate: &str) -> (Snapshot, Snapshot) {
    (
        Snapshot::prepare_independent(root, Kind::Commit(base.into())).unwrap(),
        Snapshot::prepare_independent(root, Kind::Commit(candidate.into())).unwrap(),
    )
}
pub(super) fn report(
    root: &Path,
    summary: &SourceSummary,
) -> (PathBuf, PathBuf, serde_json::Value) {
    let evidence = root.join("reports/qa");
    fs::create_dir_all(&evidence).unwrap();
    let artifact = evidence.join("screen.bin");
    fs::write(&artifact, b"\xffuninterpreted synthetic bytes").unwrap();
    let inner = json!({"version":1,"budget":"fast","build":"unverified-health-claim-even-if-it-looks-like-a-commit","scenario":"synthetic-byte-fixture","status":"PASS","journeys":crate::qa::REQUIRED.iter().map(|name|json!({"name":name,"completed":true,"evidence":[artifact]})).collect::<Vec<_>>(),"findings":[]});
    let value = json!({"version":2,"report":inner,"expected_source":{"candidate_content_witness_digest":summary.candidate_content_witness_digest,"review_subject_digest":summary.review_subject_digest}});
    let path = evidence.join("source.json");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    (path, evidence, value)
}
