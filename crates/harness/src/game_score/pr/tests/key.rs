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

/// A repository with two game crates, a crate outside the game, a lock file and a document.
fn repository() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("repository");
    let root = temp.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["config", "user.name", "Fixture"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    write(root, "crates/simulation/src/lib.rs", "pub fn tick() {}\n");
    write(root, "crates/ai/src/lib.rs", "pub fn think() {}\n");
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

fn key_at(root: &Path, rev: &str, map: &str) -> String {
    Inputs::read(root, rev, map, &inputs())
        .expect("inputs")
        .key()
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
fn the_same_inputs_give_the_same_sixty_four_digits_and_a_missing_input_is_stable() {
    let temp = repository();
    let a = key_at(temp.path(), "HEAD", "map");
    let b = key_at(temp.path(), "HEAD", "map");
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
    assert!(a.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let read = Inputs::read(temp.path(), "HEAD", "map", &inputs()).expect("inputs");
    let absent: Vec<&str> = read
        .objects
        .iter()
        .filter(|(_, id)| id.is_none())
        .map(|(path, _)| path.as_str())
        .collect();
    assert_eq!(absent, ["rust-toolchain.toml"]);
}

#[test]
fn the_order_the_paths_are_listed_in_does_not_matter() {
    let temp = repository();
    let mut reversed = inputs();
    reversed.reverse();
    let a = Inputs::read(temp.path(), "HEAD", "map", &inputs())
        .expect("a")
        .key();
    let b = Inputs::read(temp.path(), "HEAD", "map", &reversed)
        .expect("b")
        .key();
    assert_eq!(a, b);
}

#[test]
fn a_revision_git_does_not_know_is_an_error() {
    let temp = repository();
    assert!(Inputs::read(temp.path(), "nonexistent-revision", "map", &inputs()).is_err());
}

#[test]
fn a_listing_is_read_by_path() {
    let listing = "040000 tree aaa\tcrates/ai\x00100644 blob bbb\tCargo.lock\x00";
    let read = Inputs::from_listing(
        "map",
        &[
            "crates/ai".to_owned(),
            "Cargo.lock".to_owned(),
            "gone".to_owned(),
        ],
        listing,
    );
    assert_eq!(
        read.objects,
        [
            ("Cargo.lock".to_owned(), Some("bbb".to_owned())),
            ("crates/ai".to_owned(), Some("aaa".to_owned())),
            ("gone".to_owned(), None),
        ]
    );
}

#[test]
fn a_tree_that_differs_from_head_in_an_input_is_dirty() {
    let temp = repository();
    let root = temp.path();
    assert!(!dirty(root, &inputs()).expect("status"));
    write(root, "docs/guide.md", "edited\n");
    assert!(
        !dirty(root, &inputs()).expect("status"),
        "a document is not an input"
    );
    write(
        root,
        "crates/ai/src/lib.rs",
        "pub fn think() { /* edited */ }\n",
    );
    assert!(dirty(root, &inputs()).expect("status"));
    git(root, &["checkout", "--", "crates/ai"]);
    assert!(!dirty(root, &inputs()).expect("status"));
    write(root, "crates/ai/src/new.rs", "pub fn extra() {}\n");
    assert!(
        dirty(root, &inputs()).expect("status"),
        "a new file is a change"
    );
}
