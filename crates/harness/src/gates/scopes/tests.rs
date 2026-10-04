//! Fixtures mutate ONLY their own tempfile repositories.
use super::*;
mod compiler;
mod index_context;
mod metadata;
mod witness;

fn run(root: &Path, args: &[&str]) -> Vec<u8> {
    git(root, args, None).expect("Git fixture command")
}

fn put(root: &Path, name: &str, bytes: impl AsRef<[u8]>) {
    relative(name).unwrap();
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn repo() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, &["init", "--quiet", "--template="]);
    run(root, &["config", "user.name", "Fixture"]);
    run(root, &["config", "user.email", "fixture@example.invalid"]);
    run(root, &["config", "commit.gpgsign", "false"]);
    put(root, "code.rs", "pub fn original() {}\n");
    put(root, "Cargo.lock", "# original lock bytes\n");
    put(root, "fuzz/Cargo.lock", "# fuzz lock bytes\n");
    put(root, "fuzz/fuzz_targets/example.rs", "fn main() {}\n");
    put(
        root,
        ".gitignore",
        "/reports/\n/local-assets/\n/.cache/\n/target/\n",
    );
    run(root, &["add", "."]);
    run(root, &["commit", "--quiet", "-m", "base"]);
    temp
}

fn index_bytes(root: &Path) -> Vec<u8> {
    let path = line(run(
        root,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    ))
    .unwrap();
    fs::read(path).unwrap()
}

#[test]
fn clean_working_identity_is_not_an_isolation_or_trusted_judge_claim() {
    let repo = repo();
    let snapshot = Snapshot::prepare(repo.path(), Kind::Working).unwrap();
    assert!(snapshot.identity.clean_commit);
    assert!(!snapshot.identity.isolated_inputs);
    assert_eq!(snapshot.root(), fs::canonicalize(repo.path()).unwrap());
    snapshot.run_checked(|_| Ok(())).unwrap();
    put(repo.path(), "code.rs", "pub fn modified() {}\n");
    assert!(snapshot.verify_source().is_err());
    let dirty = Snapshot::prepare(repo.path(), Kind::Working).unwrap();
    assert!(!dirty.identity.clean_commit);
    assert!(dirty.identity.tree.is_none());
    put(repo.path(), "code.rs", "pub fn another_modified() {}\n");
    // Still status M: raw content fingerprint must notice the second edit.
    assert!(dirty.verify_source().is_err());
}

#[test]
fn partial_staging_checks_invalid_index_not_valid_worktree() {
    let repo = repo();
    let root = repo.path();
    put(root, "code.rs", "invalid rust\n".repeat(501));
    run(root, &["add", "code.rs"]);
    put(root, "code.rs", "pub fn valid_working_only() {}\n");
    let before = index_bytes(root);
    let refs_before = run(root, &["for-each-ref", "--format=%(refname) %(objectname)"]);
    let head_before = fs::read(root.join(".git/HEAD")).unwrap();
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    assert_ne!(snapshot.root(), root);
    assert!(!snapshot.identity.clean_commit);
    assert!(snapshot.identity.isolated_inputs);
    assert_eq!(snapshot.paths, ["code.rs"]);
    // Actual canonical structure gate rejects index bytes despite valid worktree.
    let error = snapshot
        .run_checked(|checkout| {
            crate::policy::structure(checkout).map_err(|error| Box::new(error) as Box<dyn Error>)
        })
        .unwrap_err();
    assert!(error.to_string().contains("501 lines (max 500)"));
    // A typed Clippy runner must receive this same checkout, not source root.
    snapshot
        .run_checked(|checkout| {
            assert!(syn::parse_file(&fs::read_to_string(checkout.join("code.rs"))?).is_err());
            Ok(())
        })
        .unwrap();
    assert_eq!(before, index_bytes(root));
    assert_eq!(
        refs_before,
        run(root, &["for-each-ref", "--format=%(refname) %(objectname)"])
    );
    assert_eq!(head_before, fs::read(root.join(".git/HEAD")).unwrap());
    assert!(
        fs::read_to_string(root.join("code.rs"))
            .unwrap()
            .contains("valid_working_only")
    );
}

#[test]
fn locks_fuzz_and_tracked_assets_are_index_only_no_ignored_or_untracked_copy() {
    let repo = repo();
    let root = repo.path();
    put(root, "Cargo.lock", "# staged Cargo.lock\n");
    put(root, "fuzz/Cargo.lock", "# staged fuzz lock\n");
    put(root, "assets/manifest.json", "{\"staged\":true}\n");
    run(
        root,
        &[
            "add",
            "Cargo.lock",
            "fuzz/Cargo.lock",
            "assets/manifest.json",
        ],
    );
    put(root, "Cargo.lock", "# unstaged lock\n");
    put(root, "fuzz/Cargo.lock", "# unstaged fuzz lock\n");
    put(root, "assets/manifest.json", "worktree-only asset bytes\n");
    put(root, "untracked.rs", "not part of index\n");
    put(root, "reports/forged.json", "fake evidence\n");
    put(root, "local-assets/game.dat", "original game asset\n");
    put(root, ".cache/cargo/cache.dat", "cache\n");
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    assert_eq!(
        fs::read(snapshot.root().join("Cargo.lock")).unwrap(),
        b"# staged Cargo.lock\n"
    );
    assert_eq!(
        fs::read(snapshot.root().join("fuzz/Cargo.lock")).unwrap(),
        b"# staged fuzz lock\n"
    );
    assert_eq!(
        fs::read(snapshot.root().join("assets/manifest.json")).unwrap(),
        b"{\"staged\":true}\n"
    );
    assert!(
        snapshot
            .root()
            .join("fuzz/fuzz_targets/example.rs")
            .is_file()
    );
    for excluded in ["untracked.rs", "reports", "local-assets", ".cache"] {
        assert!(!snapshot.root().join(excluded).exists(), "{excluded}");
    }
    // Index scope intentionally never probes/reads unstaged source file bytes.
    put(root, "code.rs", "totally different worktree\n");
    snapshot.verify_source().unwrap();
}

#[test]
fn staged_rename_and_deletion_use_both_names_and_never_resurrect_worktree_file() {
    let repo = repo();
    let root = repo.path();
    run(root, &["mv", "code.rs", "renamed snow ☃.rs"]);
    run(root, &["rm", "fuzz/fuzz_targets/example.rs"]);
    put(root, "code.rs", "old path restored only in worktree\n");
    put(
        root,
        "fuzz/fuzz_targets/example.rs",
        "deleted path restored only in worktree\n",
    );
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    assert_eq!(
        snapshot.paths,
        [
            "code.rs",
            "fuzz/fuzz_targets/example.rs",
            "renamed snow ☃.rs"
        ]
    );
    assert!(!snapshot.root().join("code.rs").exists());
    assert!(
        !snapshot
            .root()
            .join("fuzz/fuzz_targets/example.rs")
            .exists()
    );
    assert_eq!(
        fs::read(snapshot.root().join("renamed snow ☃.rs")).unwrap(),
        b"pub fn original() {}\n"
    );
}

#[test]
fn candidate_attributes_and_source_configuration_cannot_convert_exported_bytes() {
    let repo = repo();
    let root = repo.path();
    put(
        root,
        ".gitattributes",
        "code.rs text eol=crlf ident filter=untrusted\n",
    );
    run(root, &["add", ".gitattributes"]);
    // No shell/filter is needed in this test. Private repo must not inherit this.
    run(root, &["config", "core.autocrlf", "true"]);
    let before = index_bytes(root);
    let config = fs::read(root.join(".git/config")).unwrap();
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    assert_eq!(
        fs::read(snapshot.root().join("code.rs")).unwrap(),
        b"pub fn original() {}\n"
    );
    assert_eq!(config, fs::read(root.join(".git/config")).unwrap());
    assert_eq!(before, index_bytes(root));
}

#[test]
fn detached_snapshot_metadata_matches_index_tree() {
    let repo = repo();
    put(repo.path(), "code.rs", "pub fn staged() {}\n");
    run(repo.path(), &["add", "code.rs"]);
    let snapshot = Snapshot::prepare(repo.path(), Kind::Index).unwrap();
    let status = command(snapshot.root())
        .args(["symbolic-ref", "-q", "HEAD"])
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1), "HEAD must be detached");
    let tree = snapshot.identity.tree.as_ref().unwrap();
    assert_eq!(resolve(snapshot.root(), "HEAD", "tree").unwrap(), *tree);
    assert_eq!(line(run(snapshot.root(), &["write-tree"])).unwrap(), *tree);
    assert!(run(snapshot.root(), &["diff", "--name-only", "HEAD", "--"]).is_empty());
    assert!(run(snapshot.root(), &["for-each-ref", "--format=%(refname)"]).is_empty());
}

#[test]
fn immutable_commit_exports_requested_revision_even_with_unstaged_and_staged_changes() {
    let repo = repo();
    let root = repo.path();
    let commit = resolve(root, "HEAD", "commit").unwrap();
    put(root, "code.rs", "pub fn staged() {}\n");
    run(root, &["add", "code.rs"]);
    put(root, "code.rs", "pub fn working() {}\n");
    let before = index_bytes(root);
    let snapshot = Snapshot::prepare(root, Kind::Commit(commit.clone())).unwrap();
    assert!(snapshot.identity.clean_commit);
    assert_eq!(resolve(snapshot.root(), "HEAD", "commit").unwrap(), commit);
    assert_eq!(
        fs::read(snapshot.root().join("code.rs")).unwrap(),
        b"pub fn original() {}\n"
    );
    assert_eq!(before, index_bytes(root));
    assert!(Snapshot::prepare(root, Kind::Commit("HEAD".into())).is_err());
}

#[test]
fn changed_index_and_snapshot_mutations_invalidate_even_failed_gate_results() {
    let repo = repo();
    let root = repo.path();
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    put(root, "code.rs", "pub fn changed() {}\n");
    run(root, &["add", "code.rs"]);
    assert!(snapshot.verify_source().is_err());
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    let error = snapshot
        .run_checked::<()>(|checkout| {
            put(checkout, "code.rs", "pub fn gate_mutation() {}\n");
            Err("original gate failure".into())
        })
        .unwrap_err();
    assert!(error.to_string().contains("export altered code.rs"));
}

#[test]
fn linked_worktree_index_is_used_not_main_index_and_both_remain_byte_identical() {
    let main = repo();
    let owner = tempfile::tempdir().unwrap();
    let linked = owner.path().join("linked");
    run(
        main.path(),
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    );
    // All metadata changes above belong to the fixture. Snapshot creates none.
    put(&linked, "code.rs", "pub fn linked_index() {}\n");
    run(&linked, &["add", "code.rs"]);
    put(&linked, "code.rs", "pub fn linked_working_only() {}\n");
    let main_before = index_bytes(main.path());
    let linked_before = index_bytes(&linked);
    let snapshot = Snapshot::prepare(&linked, Kind::Index).unwrap();
    assert_eq!(
        fs::read(snapshot.root().join("code.rs")).unwrap(),
        b"pub fn linked_index() {}\n"
    );
    snapshot.run_checked(|_| Ok(())).unwrap();
    assert_eq!(main_before, index_bytes(main.path()));
    assert_eq!(linked_before, index_bytes(&linked));
    // TempDir cleanup owns both fixture roots; no computed repository deletion.
}

#[test]
fn split_index_fingerprint_and_export_are_stable() {
    let repo = repo();
    let root = repo.path();
    run(root, &["update-index", "--split-index"]);
    let before = index_bytes(root);
    let shared = line(run(root, &["rev-parse", "--shared-index-path"])).unwrap();
    assert!(!shared.is_empty());
    let shared_path = if Path::new(&shared).is_absolute() {
        PathBuf::from(shared)
    } else {
        root.join(shared)
    };
    let shared_before = fs::read(&shared_path).unwrap();
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    snapshot.run_checked(|_| Ok(())).unwrap();
    assert_eq!(before, index_bytes(root));
    assert_eq!(shared_before, fs::read(shared_path).unwrap());
}

#[test]
fn intent_to_add_and_unsafe_paths_fail_closed() {
    let repo = repo();
    put(repo.path(), "intent.rs", "pub fn pending() {}\n");
    run(repo.path(), &["add", "-N", "intent.rs"]);
    assert!(
        Snapshot::prepare(repo.path(), Kind::Index)
            .unwrap_err_text()
            .contains("intent-to-add")
    );
    for path in [
        "../escape",
        "/absolute",
        "a/../../escape",
        ".git/config",
        "a/.GIT/config",
        "a\\b",
        "C:evil",
        "a//b",
    ] {
        assert!(relative(path).is_err(), "{path}");
    }
}

#[cfg(unix)]
#[test]
fn symlink_is_rejected_before_any_export_or_candidate_target_read() {
    let repo = repo();
    std::os::unix::fs::symlink("../../outside", repo.path().join("candidate-link")).unwrap();
    run(repo.path(), &["add", "candidate-link"]);
    let before = index_bytes(repo.path());
    assert!(
        Snapshot::prepare(repo.path(), Kind::Index)
            .unwrap_err_text()
            .contains("no symlinks/gitlinks")
    );
    assert_eq!(before, index_bytes(repo.path()));
}

// Result::unwrap_err requires Snapshot: Debug (TempDir ownership is not evidence).
trait ErrorText {
    fn unwrap_err_text(self) -> String;
}
impl ErrorText for Result<Snapshot> {
    fn unwrap_err_text(self) -> String {
        match self {
            Ok(_) => panic!("expected rejected scope"),
            Err(error) => error.to_string(),
        }
    }
}
