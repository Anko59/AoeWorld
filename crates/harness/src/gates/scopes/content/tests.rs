//! Real isolated Git fixtures only; no global environment edits or source resets.
use super::*;

fn run(root: &Path, args: &[&str]) -> Vec<u8> {
    git(root, args, None).unwrap()
}
fn text(root: &Path, args: &[&str]) -> String {
    line(run(root, args)).unwrap()
}
fn commit(root: &Path, parent: Option<&str>) -> String {
    let tree = text(root, &["write-tree"]);
    let parent = parent.map_or(String::new(), |parent| format!("parent {parent}\n"));
    let raw = format!(
        "tree {tree}\n{parent}author Fixture <fixture@example.invalid> 0 +0000\ncommitter Fixture <fixture@example.invalid> 0 +0000\n\nFixed content witness fixture.\n"
    );
    let commit = line(
        git(
            root,
            &["hash-object", "-w", "-t", "commit", "--stdin"],
            Some(raw.as_bytes()),
        )
        .unwrap(),
    )
    .unwrap();
    run(root, &["update-ref", "HEAD", &commit]);
    commit
}
fn repo() -> (TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    run(root, &["init", "--quiet", "--template="]);
    fs::write(root.join(".gitignore"), "/.cache/\n/reports/\n/target/\n").unwrap();
    fs::write(root.join("code.rs"), "pub fn original() {}\n").unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/other.rs"), "pub fn original() {}\n").unwrap();
    run(root, &["add", "--all"]);
    let selected = commit(root, None);
    (repo, selected)
}
fn snapshot(root: &Path, kind: Kind) -> Snapshot {
    Snapshot::prepare_independent(root, kind).unwrap()
}
fn alternate_index(root: &Path) -> PathBuf {
    let alternate = root.join(".git/content-alternate-index");
    assert!(!alternate.exists());
    fs::copy(root.join(".git/index"), &alternate).unwrap();
    git_at(
        root,
        Some(&alternate),
        &["update-index", "--index-version=4"],
        None,
    )
    .unwrap();
    assert_ne!(
        fs::read(root.join(".git/index")).unwrap(),
        fs::read(&alternate).unwrap()
    );
    alternate
}
fn loose(root: &Path, object: &str) -> PathBuf {
    oid(object).unwrap();
    let path = root
        .join(".git/objects")
        .join(&object[..2])
        .join(&object[2..]);
    assert_eq!(fs::canonicalize(&path).unwrap(), path);
    assert!(path.is_file());
    path
}
fn replace_blob(root: &Path, original: &str) {
    let other = line(
        git(
            root,
            &["hash-object", "-w", "--stdin"],
            Some(b"different raw object bytes\n"),
        )
        .unwrap(),
    )
    .unwrap();
    let target = loose(root, original);
    let other = loose(root, &other);
    // Resolve the exact private-fixture target before deletion, not a broad tree.
    assert_eq!(fs::canonicalize(&target).unwrap(), target);
    fs::remove_file(&target).unwrap();
    fs::copy(other, target).unwrap();
}
fn blobs(witness: &ContentWitness) -> Vec<(String, String)> {
    witness
        .objects
        .iter()
        .filter(|object| object.kind == ObjectKind::Blob)
        .map(|object| (object.oid.clone(), object.raw_blake3.clone()))
        .collect()
}
#[test]
fn selected_commit_is_stable_across_repositories_index_paths_and_fresh_exports() {
    let (first, selected) = repo();
    let (second, same_selected) = repo();
    assert_eq!(selected, same_selected);
    let first_snapshot = snapshot(first.path(), Kind::Commit(selected.clone()));
    let alternate = alternate_index(second.path());
    let second_snapshot = Snapshot::prepare_index(
        second.path(),
        Kind::Commit(selected.clone()),
        Some(&alternate),
    )
    .unwrap();
    assert_ne!(first_snapshot.root(), second_snapshot.root());
    assert_ne!(
        first_snapshot.identity.effective_index,
        second_snapshot.identity.effective_index
    );
    let witness = first_snapshot.content_witness().unwrap();
    assert_eq!(second_snapshot.content_witness().unwrap(), witness);
    assert_eq!(
        witness.kind,
        ContentKind::Commit {
            commit: selected.clone(),
            tree: first_snapshot.identity.tree.clone().unwrap()
        }
    );
    assert_eq!(witness.version, 1);
    assert_eq!(witness.algorithm, ALGORITHM);
    assert_eq!(witness.digest.len(), 64);
    assert!(
        witness
            .files
            .windows(2)
            .all(|pair| pair[0].path < pair[1].path)
    );
    let serialized = serde_json::to_string(&witness).unwrap();
    for path in [
        first.path(),
        second.path(),
        first_snapshot.root(),
        second_snapshot.root(),
        &alternate,
    ] {
        assert!(!serialized.contains(path.to_str().unwrap()));
    }
    assert!(!serialized.contains("index_fingerprint"));
    assert!(!serialized.contains("effective_index"));
}
#[test]
fn selected_commit_stays_stable_after_unrelated_head_changes_between_fresh_operations() {
    let (repo, selected) = repo();
    let first = snapshot(repo.path(), Kind::Commit(selected.clone()));
    let expected = first.content_witness().unwrap();
    fs::write(repo.path().join("code.rs"), "pub fn later_head() {}\n").unwrap();
    run(repo.path(), &["add", "code.rs"]);
    let changed_head = commit(repo.path(), Some(&selected));
    assert_ne!(changed_head, selected);
    assert!(
        first.content_witness().is_err(),
        "original endpoint observations must still reject changed HEAD"
    );
    let fresh = snapshot(repo.path(), Kind::Commit(selected));
    assert_eq!(fresh.content_witness().unwrap(), expected);
    assert!(
        !fresh
            .content_witness()
            .unwrap()
            .objects
            .iter()
            .any(|object| object.oid == changed_head && object.kind == ObjectKind::Commit)
    );
}
#[test]
fn index_subject_uses_logical_entries_not_physical_index_path_or_private_synthetic_commit() {
    let (repo, selected) = repo();
    fs::write(repo.path().join("code.rs"), "pub fn staged() {}\n").unwrap();
    run(repo.path(), &["add", "code.rs"]);
    let first = snapshot(repo.path(), Kind::Index);
    let alternate = alternate_index(repo.path());
    let second = Snapshot::prepare_index(repo.path(), Kind::Index, Some(&alternate)).unwrap();
    let witness = first.content_witness().unwrap();
    assert_eq!(second.content_witness().unwrap(), witness);
    let tree = first.identity.tree.clone().unwrap();
    assert_eq!(
        witness.kind,
        ContentKind::Index {
            source_head: selected.clone(),
            pending_tree: tree.clone()
        }
    );
    assert!(
        git(repo.path(), &["cat-file", "tree", &tree], None).is_err(),
        "pending tree was generated privately, never required in source"
    );
    assert!(git(first.root(), &["cat-file", "tree", &tree], None).is_ok());
    let synthetic = text(first.root(), &["rev-parse", "HEAD"]);
    assert_ne!(synthetic, selected);
    assert!(
        !witness
            .objects
            .iter()
            .any(|object| object.oid == synthetic && object.kind == ObjectKind::Commit)
    );
    assert!(
        witness
            .objects
            .iter()
            .any(|object| object.oid == selected && object.kind == ObjectKind::Commit)
    );
}
#[test]
fn paths_and_modes_bind_the_same_raw_blob_set_differently() {
    let (repo, _) = repo();
    let original = snapshot(repo.path(), Kind::Index)
        .content_witness()
        .unwrap();
    let original_path = repo.path().join("code.rs");
    let renamed_path = repo.path().join("renamed.rs");
    assert_eq!(fs::canonicalize(&original_path).unwrap(), original_path);
    assert_eq!(
        fs::canonicalize(renamed_path.parent().unwrap()).unwrap(),
        repo.path()
    );
    assert!(!renamed_path.exists());
    run(repo.path(), &["mv", "code.rs", "renamed.rs"]);
    let renamed = snapshot(repo.path(), Kind::Index)
        .content_witness()
        .unwrap();
    assert_eq!(blobs(&original), blobs(&renamed));
    assert_ne!(original.digest, renamed.digest);
    assert!(renamed.files.iter().any(|file| file.path == "renamed.rs"));
    run(repo.path(), &["update-index", "--chmod=+x", "renamed.rs"]);
    let executable = snapshot(repo.path(), Kind::Index)
        .content_witness()
        .unwrap();
    assert_eq!(blobs(&renamed), blobs(&executable));
    assert_ne!(renamed.digest, executable.digest);
    assert!(
        executable
            .files
            .iter()
            .any(|file| file.path == "renamed.rs" && file.mode == FileMode::Executable)
    );
}
#[test]
fn selected_tree_closure_is_not_ancestor_history_closure() {
    let (repo, parent) = repo();
    fs::write(repo.path().join("code.rs"), "pub fn child() {}\n").unwrap();
    run(repo.path(), &["add", "code.rs"]);
    let child = commit(repo.path(), Some(&parent));
    let snapshot = snapshot(repo.path(), Kind::Commit(child.clone()));
    let witness = snapshot.content_witness().unwrap();
    assert!(git(snapshot.root(), &["cat-file", "commit", &parent], None).is_err());
    assert!(git(snapshot.root(), &["cat-file", "commit", &child], None).is_ok());
    assert_eq!(
        witness
            .objects
            .iter()
            .filter(|object| object.kind == ObjectKind::Commit)
            .count(),
        1
    );
    assert!(
        !witness
            .objects
            .iter()
            .any(|object| object.oid == parent && object.kind == ObjectKind::Commit)
    );
    let raw = git(snapshot.root(), &["cat-file", "commit", &child], None).unwrap();
    assert!(
        String::from_utf8(raw)
            .unwrap()
            .contains(&format!("parent {parent}\n"))
    );
}
#[test]
fn source_blob_replacement_under_the_same_oid_is_rejected_for_both_immutable_kinds() {
    for index in [false, true] {
        let (repo, selected) = repo();
        let kind = if index {
            Kind::Index
        } else {
            Kind::Commit(selected)
        };
        let snapshot = snapshot(repo.path(), kind);
        let witness = snapshot.content_witness().unwrap();
        let object = witness
            .files
            .iter()
            .find(|file| file.path == "code.rs")
            .unwrap()
            .blob_oid
            .clone();
        let before = fs::read(repo.path().join(".git/index")).unwrap();
        replace_blob(repo.path(), &object);
        assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), before);
        assert!(snapshot.content_witness().is_err());
    }
}
#[test]
fn private_raw_blob_comparison_is_required_even_if_physical_metadata_is_reobserved() {
    for index in [false, true] {
        let (repo, selected) = repo();
        let kind = if index {
            Kind::Index
        } else {
            Kind::Commit(selected)
        };
        let mut snapshot = snapshot(repo.path(), kind);
        let witness = snapshot.content_witness().unwrap();
        let original = witness
            .files
            .iter()
            .find(|file| file.path == "code.rs")
            .unwrap()
            .blob_oid
            .clone();
        replace_blob(snapshot.root(), &original);
        assert!(
            snapshot.content_witness().is_err(),
            "original metadata endpoint remains enforced"
        );
        // Private unit-fixture seam only: even a newly observed physical seal is
        // not the captured source raw-byte proof. No public re-sealing API exists.
        snapshot.metadata = Some(metadata_seal(snapshot.root(), None).unwrap());
        let error = snapshot.content_witness().unwrap_err().to_string();
        assert!(
            error.contains("raw object content") || error.contains("private index blob content"),
            "{error}"
        );
    }
}
#[test]
fn worktree_bytes_and_executable_mode_mutations_are_not_content_witnesses() {
    let (repo, selected) = repo();
    let snapshot = snapshot(repo.path(), Kind::Commit(selected.clone()));
    fs::write(
        snapshot.root().join("code.rs"),
        "pub fn forged_worktree() {}\n",
    )
    .unwrap();
    assert!(snapshot.content_witness().is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let snapshot =
            super::Snapshot::prepare_independent(repo.path(), Kind::Commit(selected)).unwrap();
        fs::set_permissions(
            snapshot.root().join("code.rs"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert!(snapshot.content_witness().is_err());
    }
}
#[test]
fn working_is_rejected_and_canonical_payload_excludes_its_generated_digest() {
    let (repo, _) = repo();
    let working = snapshot(repo.path(), Kind::Working);
    assert!(
        working
            .content_witness()
            .unwrap_err()
            .to_string()
            .contains("immutable Index or Commit")
    );
    let witness = snapshot(repo.path(), Kind::Index)
        .content_witness()
        .unwrap();
    let bytes = serde_json::to_vec(&Payload {
        version: witness.version,
        algorithm: &witness.algorithm,
        kind: &witness.kind,
        files: &witness.files,
        objects: &witness.objects,
    })
    .unwrap();
    let mut hash = blake3::Hasher::new();
    hash.update(DOMAIN);
    hash.update(&bytes);
    assert_eq!(hash.finalize().to_hex().to_string(), witness.digest);
    assert!(!String::from_utf8(bytes).unwrap().contains("digest"));
    assert!(FileMode::parse("120000").is_err());
    assert!(ObjectKind::parse("tag").is_err());
    let duplicate = b"100644 0000000000000000000000000000000000000000 0\tcode.rs\0".repeat(2);
    assert!(inventory(&duplicate).is_err());
}
#[test]
fn actual_scope_check_binary_reports_non_authoritative_content_witness() {
    let (repo, selected) = repo();
    let executable = std::env::current_exe().unwrap();
    let profile = executable.parent().unwrap().parent().unwrap();
    let binary = profile.join(if cfg!(windows) {
        "aoe-harness.exe"
    } else {
        "aoe-harness"
    });
    assert!(
        binary.is_file(),
        "canonical test-harness target must build its CLI binary"
    );
    for commit in [false, true] {
        let mut command = Command::new(&binary);
        command.current_dir(repo.path()).arg("scope-check");
        for (name, _) in std::env::vars_os() {
            if name.as_encoded_bytes().starts_with(b"GIT_") {
                command.env_remove(name);
            }
        }
        if commit {
            command.args(["--revision", &selected]);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["authoritative"], false);
        let kind = if commit {
            Kind::Commit(selected.clone())
        } else {
            Kind::Index
        };
        let witness = snapshot(repo.path(), kind).content_witness().unwrap();
        assert_eq!(
            report["content_witness"],
            serde_json::to_value(witness).unwrap()
        );
    }
}
