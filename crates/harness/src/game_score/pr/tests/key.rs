use super::*;
use std::fs;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().expect("parent")).expect("directory");
    fs::write(full, text).expect("file");
}

fn commit(root: &Path, message: &str) -> String {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
    git(root, &["rev-parse", "HEAD"])
}

/// A repository with two game crates (one with a test and a document), a crate outside the game,
/// a lock file and a document.
fn repository() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("repository");
    let root = temp.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["config", "user.name", "Fixture"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    write(root, "crates/simulation/src/lib.rs", "pub fn tick() {}\n");
    write(root, "crates/ai/src/lib.rs", "pub fn think() {}\n");
    write(root, "crates/ai/tests/it.rs", "#[test]\nfn thinks() {}\n");
    write(root, "crates/ai/README.md", "the ai\n");
    write(root, "crates/client/src/lib.rs", "pub fn draw() {}\n");
    write(root, "Cargo.lock", "# lock\n");
    write(root, "docs/guide.md", "words\n");
    commit(root, "one");
    temp
}

fn inputs() -> Vec<String> {
    [
        "crates/simulation",
        "crates/ai",
        "Cargo.lock",
        "rust-toolchain.toml",
    ]
    .map(str::to_owned)
    .to_vec()
}

/// What `ignore` of the rule says, in short: the tests and the documents of a crate.
fn ignored(path: &str) -> bool {
    path.ends_with(".md") || path.contains("/tests/")
}

fn read_at(root: &Path, rev: &str, map: &str, paths: &[String]) -> Inputs {
    Inputs::read(root, rev, map, paths, &ignored).expect("inputs")
}

fn key_at(root: &Path, rev: &str, map: &str) -> String {
    read_at(root, rev, map, &inputs()).key()
}

#[test]
fn the_key_does_not_depend_on_the_commit_only_on_what_the_game_is_made_of() {
    let temp = repository();
    let root = temp.path();
    let first = git(root, &["rev-parse", "HEAD"]);
    write(root, "docs/guide.md", "other words\n");
    write(root, "crates/client/src/lib.rs", "pub fn paint() {}\n");
    let second = commit(root, "a document and a crate outside the game");
    assert_ne!(first, second);
    assert_eq!(key_at(root, &first, "map"), key_at(root, &second, "map"));
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]);
    let same_tree = git(
        root,
        &["commit-tree", &tree, "-m", "same tree, another commit"],
    );
    assert_ne!(same_tree, second);
    assert_eq!(
        key_at(root, &same_tree, "map"),
        key_at(root, &second, "map")
    );
}

#[test]
fn a_change_in_a_game_crate_the_lock_or_the_map_changes_the_key() {
    let temp = repository();
    let root = temp.path();
    let before = key_at(root, "HEAD", "map");
    assert_ne!(before, key_at(root, "HEAD", "another map"));
    write(
        root,
        "crates/simulation/src/lib.rs",
        "pub fn tick() { /* faster */ }\n",
    );
    let simulation = commit(root, "simulation");
    assert_ne!(before, key_at(root, &simulation, "map"));
    write(root, "Cargo.lock", "# another lock\n");
    let lock = commit(root, "lock");
    assert_ne!(key_at(root, &simulation, "map"), key_at(root, &lock, "map"));
    write(root, "rust-toolchain.toml", "[toolchain]\n");
    let toolchain = commit(root, "toolchain");
    assert_ne!(
        key_at(root, &lock, "map"),
        key_at(root, &toolchain, "map"),
        "an input that did not exist"
    );
}

#[test]
fn the_same_inputs_give_the_same_sixty_four_digits_and_list_every_file_that_counts() {
    let temp = repository();
    let a = key_at(temp.path(), "HEAD", "map");
    let b = key_at(temp.path(), "HEAD", "map");
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
    assert!(a.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let read = read_at(temp.path(), "HEAD", "map", &inputs());
    let listed: Vec<&str> = read.objects.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        listed,
        [
            "Cargo.lock",
            "crates/ai/src/lib.rs",
            "crates/simulation/src/lib.rs"
        ],
        "files, not directories; no test, no document; no toolchain file here"
    );
}

#[test]
fn a_test_or_a_document_of_a_game_crate_does_not_change_the_key_and_its_code_does() {
    let temp = repository();
    let root = temp.path();
    let before = key_at(root, "HEAD", "map");
    write(
        root,
        "crates/ai/tests/it.rs",
        "#[test]\nfn thinks_more() {}\n",
    );
    write(root, "crates/ai/tests/another.rs", "#[test]\nfn new() {}\n");
    write(root, "crates/ai/README.md", "the ai, explained\n");
    let documented = commit(root, "a test and a document of the ai");
    assert_eq!(
        before,
        key_at(root, &documented, "map"),
        "the selection says these do not run the check: they are not the game"
    );
    write(root, "crates/ai/src/think.rs", "pub fn more() {}\n");
    let code = commit(root, "a new source file of the ai");
    assert_ne!(before, key_at(root, &code, "map"), "a file added");
    git(root, &["rm", "-q", "crates/ai/src/think.rs"]);
    let removed = commit(root, "and gone again");
    assert_eq!(before, key_at(root, &removed, "map"), "a file removed");
}

#[test]
fn the_order_the_paths_are_listed_in_does_not_matter() {
    let temp = repository();
    let mut reversed = inputs();
    reversed.reverse();
    let a = read_at(temp.path(), "HEAD", "map", &inputs()).key();
    let b = read_at(temp.path(), "HEAD", "map", &reversed).key();
    assert_eq!(a, b);
}

#[test]
fn a_revision_git_does_not_know_is_an_error() {
    let temp = repository();
    assert!(
        Inputs::read(
            temp.path(),
            "nonexistent-revision",
            "map",
            &inputs(),
            &ignored
        )
        .is_err()
    );
}

#[test]
fn a_listing_is_read_by_path_without_what_is_ignored() {
    let listing = "100644 blob bbb\tcrates/ai/src/lib.rs\x00100644 blob ccc\tCargo.lock\x00\
                   100644 blob ddd\tcrates/ai/README.md\x00160000 commit eee\tvendor/dep\x00";
    let read = Inputs::from_listing("map", listing, &ignored);
    assert_eq!(
        read.objects,
        [
            ("Cargo.lock".to_owned(), "ccc".to_owned()),
            ("crates/ai/src/lib.rs".to_owned(), "bbb".to_owned()),
            ("vendor/dep".to_owned(), "eee".to_owned()),
        ]
    );
}

fn dirty_now(root: &Path) -> bool {
    dirty(root, &inputs(), &ignored).expect("status")
}

#[test]
fn a_tree_that_differs_from_head_in_an_input_is_dirty() {
    let temp = repository();
    let root = temp.path();
    assert!(!dirty_now(root));
    write(root, "docs/guide.md", "edited\n");
    assert!(!dirty_now(root), "a document is not an input");
    write(
        root,
        "crates/ai/src/lib.rs",
        "pub fn think() { /* edited */ }\n",
    );
    assert!(dirty_now(root));
    git(root, &["checkout", "--", "crates/ai"]);
    assert!(!dirty_now(root));
    write(root, "crates/ai/src/new.rs", "pub fn extra() {}\n");
    assert!(dirty_now(root), "a new file is a change");
}

#[test]
fn a_test_or_a_document_of_a_game_crate_does_not_make_the_tree_dirty() {
    let temp = repository();
    let root = temp.path();
    write(root, "crates/ai/tests/it.rs", "#[test]\nfn edited() {}\n");
    write(root, "crates/ai/tests/new.rs", "#[test]\nfn new() {}\n");
    write(root, "crates/ai/README.md", "edited\n");
    assert!(!dirty_now(root), "the key leaves them out");
    git(
        root,
        &["mv", "crates/ai/tests/it.rs", "crates/ai/tests/moved.rs"],
    );
    assert!(!dirty_now(root), "a rename between two ignored paths");
}

#[test]
fn a_file_moved_out_of_the_game_into_a_test_folder_is_dirty() {
    let temp = repository();
    let root = temp.path();
    git(
        root,
        &["mv", "crates/ai/src/lib.rs", "crates/ai/tests/lib.rs"],
    );
    assert!(dirty_now(root), "the game lost a file");
}
