use super::*;
use std::cell::{Cell, RefCell};

#[derive(Default)]
struct FakeRuntime {
    calls: RefCell<Vec<Vec<String>>>,
    dirty: Cell<bool>,
    wrong_branch: Cell<bool>,
    fail_copy: Cell<bool>,
}

impl Runtime for FakeRuntime {
    fn output(&self, program: &str, args: &[&str]) -> Result<String> {
        self.calls.borrow_mut().push(
            std::iter::once(program)
                .chain(args.iter().copied())
                .map(str::to_owned)
                .collect(),
        );
        match (program, args) {
            ("git", ["rev-parse", "HEAD"]) => Ok("a".repeat(40)),
            ("git", ["rev-parse", value]) if value.ends_with("^{tree}") => Ok("b".repeat(40)),
            ("git", ["status", "--porcelain"]) => {
                Ok(if self.dirty.get() { "modified" } else { "" }.to_owned())
            }
            ("git", ["branch", "--show-current"]) => Ok(if self.wrong_branch.get() {
                "feature"
            } else {
                "dev"
            }
            .to_owned()),
            ("docker", ["create", ..]) => Ok("temporary-container".to_owned()),
            ("docker", ["image", "inspect", .., tag]) if tag.contains("/server:") => {
                Ok("sha256:server".to_owned())
            }
            ("docker", ["image", "inspect", .., tag]) if tag.contains("/browser:") => {
                Ok("sha256:browser".to_owned())
            }
            ("rustc", ["--version"]) => Ok("rustc 1.93.1".to_owned()),
            _ => Err(format!("unexpected output command: {program} {args:?}").into()),
        }
    }

    fn checked(&self, program: &str, args: &[&str]) -> Result<()> {
        self.calls.borrow_mut().push(
            std::iter::once(program)
                .chain(args.iter().copied())
                .map(str::to_owned)
                .collect(),
        );
        if program != "docker" {
            return Err("unexpected checked program".into());
        }
        if let ["cp", _, directory] = args {
            if self.fail_copy.get() {
                return Err("injected copy failure".into());
            }
            fs::write(Path::new(directory).join("index.html"), b"release bundle")?;
        }
        Ok(())
    }
}

fn release_evidence(root: &Path) {
    fs::create_dir_all(root.join("reports/e2e")).expect("E2E directory");
    fs::create_dir_all(root.join("reports/perf")).expect("performance directory");
    let revision = "a".repeat(40);
    fs::write(
        root.join("reports/e2e/pass.json"),
        serde_json::to_vec(&serde_json::json!({"revision":revision,"result":"PASS"}))
            .expect("JSON"),
    )
    .expect("E2E report");
    fs::write(
        root.join("reports/perf/ci.json"),
        serde_json::to_vec(
            &serde_json::json!({"revision":revision,"verdict":"PASS","dirty":false}),
        )
        .expect("JSON"),
    )
    .expect("performance report");
}

#[test]
fn release_build_publishes_one_immutable_bundle_and_verifies_all_evidence() {
    let temp = tempfile::tempdir().expect("checkout");
    release_evidence(temp.path());
    let runtime = FakeRuntime::default();
    build_with(&runtime, temp.path()).expect("release build");
    let manifest_path = temp
        .path()
        .join("reports/release")
        .join("a".repeat(40))
        .join("manifest.json");
    let manifest = load_and_verify_with(&runtime, &manifest_path).expect("verified release");
    assert_eq!(manifest.server_image_id, "sha256:server");
    assert_eq!(manifest.browser_image_id, "sha256:browser");
    assert_eq!(
        runtime
            .calls
            .borrow()
            .iter()
            .filter(|args| args.get(1).is_some_and(|arg| arg == "build"))
            .count(),
        3
    );
    assert!(build_with(&runtime, temp.path()).is_err());
    fs::write(
        manifest_path.parent().expect("parent").join("perf.json"),
        b"tamper",
    )
    .expect("tamper");
    assert!(load_and_verify_with(&runtime, &manifest_path).is_err());
}

#[test]
fn release_build_rejects_dirty_or_wrong_branch_and_cleans_failed_staging() {
    let temp = tempfile::tempdir().expect("checkout");
    release_evidence(temp.path());
    let runtime = FakeRuntime::default();
    runtime.dirty.set(true);
    assert!(build_with(&runtime, temp.path()).is_err());
    runtime.dirty.set(false);
    runtime.wrong_branch.set(true);
    assert!(build_with(&runtime, temp.path()).is_err());
    runtime.wrong_branch.set(false);
    runtime.fail_copy.set(true);
    assert!(build_with(&runtime, temp.path()).is_err());
    let release_dir = temp.path().join("reports/release");
    assert_eq!(
        fs::read_dir(&release_dir)
            .expect("release directory")
            .count(),
        0
    );
    assert!(runtime.calls.borrow().iter().any(|args| {
        args.get(1).is_some_and(|arg| arg == "rm")
            && args.iter().any(|arg| arg == "temporary-container")
    }));
}

#[test]
fn bundle_hash_changes_on_tamper() {
    let directory = tempfile::tempdir().expect("tempdir");
    fs::write(directory.path().join("index.html"), "original").expect("write");
    let first = hash_bundle(directory.path()).expect("hash");
    fs::write(directory.path().join("index.html"), "tampered").expect("write");
    assert_ne!(first, hash_bundle(directory.path()).expect("hash"));
}

#[test]
fn bundle_inventory_rejects_empty_and_symlinked_content() {
    let empty = tempfile::tempdir().expect("directory");
    assert!(hash_bundle(empty.path()).is_err());
    let first = tempfile::tempdir().expect("first");
    let second = tempfile::tempdir().expect("second");
    for directory in [first.path(), second.path()] {
        fs::create_dir(directory.join("nested")).expect("nested");
    }
    fs::write(first.path().join("index.html"), b"home").expect("first file");
    fs::write(first.path().join("nested/app.js"), b"app").expect("nested file");
    fs::write(second.path().join("nested/app.js"), b"app").expect("nested file");
    fs::write(second.path().join("index.html"), b"home").expect("second file");
    assert_eq!(
        hash_bundle(first.path()).expect("first hash"),
        hash_bundle(second.path()).expect("second hash")
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("index.html", second.path().join("alias.html"))
            .expect("symlink");
        assert!(hash_bundle(second.path()).is_err());
    }
}

#[test]
fn release_evidence_rejects_wrong_revision_status_and_dirty_performance() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("evidence.json");
    fs::write(&path, br#"{"revision":"abc","result":"PASS"}"#).expect("E2E evidence");
    assert_eq!(
        evidence(&path, "abc", "result").expect("valid hash").len(),
        64
    );
    assert!(evidence(&path, "other", "result").is_err());
    fs::write(&path, br#"{"revision":"abc","result":"FAIL"}"#).expect("E2E fail");
    assert!(evidence(&path, "abc", "result").is_err());
    fs::write(
        &path,
        br#"{"revision":"abc","verdict":"PASS","dirty":true}"#,
    )
    .expect("dirty performance");
    assert!(evidence(&path, "abc", "verdict").is_err());
    fs::write(
        &path,
        br#"{"revision":"abc","verdict":"PASS","dirty":false}"#,
    )
    .expect("clean performance");
    assert!(evidence(&path, "abc", "verdict").is_ok());
}

#[test]
fn release_manifest_checks_format_tree_and_bundle_before_images() {
    let directory = tempfile::tempdir().expect("directory");
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).expect("bundle");
    fs::write(bundle.join("index.html"), b"original").expect("bundle file");
    let revision = output("git", &["rev-parse", "HEAD"]).expect("revision");
    let tree = output("git", &["rev-parse", "HEAD^{tree}"]).expect("tree");
    let mut manifest = Manifest {
        version: 2,
        source_commit: revision,
        source_tree: tree,
        server_image_id: "sha256:server".to_owned(),
        browser_image_id: "sha256:browser".to_owned(),
        bundle_hash: hash_bundle(&bundle).expect("bundle hash"),
        protocol_version: aoe_protocol::VERSION,
        asset_pack_version: 1,
        rustc: "test".to_owned(),
        e2e_report_hash: "a".repeat(64),
        perf_report_hash: "b".repeat(64),
    };
    let path = directory.path().join("manifest.json");
    let write = |manifest: &Manifest| {
        fs::write(&path, serde_json::to_vec(manifest).expect("JSON")).expect("manifest")
    };
    write(&manifest);
    assert!(load_and_verify(&path).is_err());
    manifest.version = 1;
    manifest.source_commit = "invalid".to_owned();
    write(&manifest);
    assert!(load_and_verify(&path).is_err());
    manifest.source_commit = output("git", &["rev-parse", "HEAD"]).expect("revision");
    manifest.source_tree = "wrong".to_owned();
    write(&manifest);
    assert!(load_and_verify(&path).is_err());
    manifest.source_tree = output("git", &["rev-parse", "HEAD^{tree}"]).expect("tree");
    fs::write(bundle.join("index.html"), b"tampered").expect("tamper");
    write(&manifest);
    assert!(load_and_verify(&path).is_err());
}
