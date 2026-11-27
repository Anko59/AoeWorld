use super::*;

fn git(directory: &Path, args: &[&str]) -> String {
    super::git(directory, args).unwrap_or_else(|error| panic!("{error}"))
}

fn commit(repo: &Path, text: &str) -> String {
    fs::write(repo.join("game.txt"), text).expect("file");
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", text]);
    git(repo, &["rev-parse", "HEAD"])
}

/// A repository of two commits, `target/` ignored, and an empty home.
fn fixture() -> (tempfile::TempDir, tempfile::TempDir, String, String) {
    let repo = tempfile::tempdir().expect("repository");
    git(repo.path(), &["init", "-q"]);
    git(
        repo.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    git(repo.path(), &["config", "user.name", "Fixture"]);
    git(repo.path(), &["config", "commit.gpgsign", "false"]);
    fs::write(repo.path().join(".gitignore"), "target/\n").expect("ignore");
    let first = commit(repo.path(), "one");
    let second = commit(repo.path(), "two");
    (repo, tempfile::tempdir().expect("home"), first, second)
}

fn text(checkout: &Path) -> String {
    fs::read_to_string(checkout.join("game.txt")).expect("game.txt")
}

#[test]
fn the_scratch_worktree_is_made_detached_at_the_merge_base() {
    let (repo, home, first, _) = fixture();
    let checkout = prepare(repo.path(), home.path(), &first).expect("prepared");
    assert_eq!(checkout, home.path().join("base"));
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), first);
    assert_eq!(text(&checkout), "one");
    assert!(
        super::git(&checkout, &["symbolic-ref", "-q", "HEAD"]).is_err(),
        "HEAD is detached"
    );
    let listed = git(repo.path(), &["worktree", "list", "--porcelain"]);
    assert!(
        listed.contains(&*fs::canonicalize(&checkout).expect("path").to_string_lossy()),
        "{listed}"
    );
}

#[test]
fn it_moves_to_the_next_merge_base_and_keeps_what_git_ignores() {
    let (repo, home, first, second) = fixture();
    let checkout = prepare(repo.path(), home.path(), &first).expect("first");
    fs::create_dir_all(checkout.join("target")).expect("target");
    fs::write(checkout.join("target/built"), "compiled").expect("build output");
    fs::write(checkout.join("leftover.txt"), "stray").expect("stray");
    let moved = prepare(repo.path(), home.path(), &second).expect("second");
    assert_eq!(moved, checkout);
    assert_eq!(git(&moved, &["rev-parse", "HEAD"]), second);
    assert_eq!(text(&moved), "two");
    assert!(
        moved.join("target/built").is_file(),
        "the next build is incremental"
    );
    assert!(!moved.join("leftover.txt").exists(), "a stray file goes");
}

#[test]
fn local_changes_in_it_are_discarded() {
    let (repo, home, first, second) = fixture();
    let checkout = prepare(repo.path(), home.path(), &first).expect("first");
    fs::write(checkout.join("game.txt"), "edited by hand").expect("edit");
    prepare(repo.path(), home.path(), &second).expect("second");
    assert_eq!(text(&checkout), "two");
}

#[test]
fn a_worktree_the_repository_forgot_is_made_again() {
    let (repo, home, first, second) = fixture();
    prepare(repo.path(), home.path(), &first).expect("first");
    fs::remove_dir_all(repo.path().join(".git/worktrees")).expect("registration lost");
    let checkout = prepare(repo.path(), home.path(), &second).expect("again");
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), second);
}

#[test]
fn a_directory_that_is_not_a_worktree_of_this_repository_is_replaced() {
    let (repo, home, first, _) = fixture();
    let base = home.path().join("base");
    fs::create_dir_all(&base).expect("directory");
    fs::write(base.join("stray.txt"), "x").expect("stray");
    git(&base, &["init", "-q"]);
    let checkout = prepare(repo.path(), home.path(), &first).expect("prepared");
    assert_eq!(text(&checkout), "one");
    assert!(!checkout.join("stray.txt").exists());
}

#[test]
fn an_unknown_revision_is_an_error() {
    let (repo, home, _, _) = fixture();
    assert!(prepare(repo.path(), home.path(), "0".repeat(40).as_str()).is_err());
}

/// The administrative entries of the repository's worktrees: `(entry name, the worktree's .git)`.
fn entries(repo: &Path) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = fs::read_dir(repo.join(".git/worktrees"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read_to_string(entry.path().join("gitdir"))
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
            )
        })
        .collect();
    found.sort();
    found
}

/// A second worktree of `repo` whose folder is then moved out of sight, as every other worktree is
/// to a container that mounts only its own checkout and the common Git directory.
fn invisible_worktree(repo: &Path, parent: &Path) -> (String, PathBuf) {
    let folder = parent.join("someone-elses-worktree");
    git(
        repo,
        &["worktree", "add", "--detach", &folder.to_string_lossy()],
    );
    let entry = entries(repo)
        .into_iter()
        .find(|(_, gitdir)| Path::new(gitdir) == folder.join(".git"))
        .map(|(name, _)| name)
        .expect("the entry of the other worktree");
    let away = parent.join("moved-away");
    fs::rename(&folder, &away).expect("moved out of sight");
    (entry, away)
}

#[test]
fn a_miss_leaves_the_registration_of_every_worktree_it_cannot_see() {
    let (repo, home, first, _) = fixture();
    let other = tempfile::tempdir().expect("elsewhere");
    let (entry, _away) = invisible_worktree(repo.path(), other.path());
    prepare(repo.path(), home.path(), &first).expect("a miss makes the scratch worktree");
    let names: Vec<String> = entries(repo.path())
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(
        names.contains(&entry),
        "the entry of a worktree whose folder is not visible must stay: {names:?}"
    );
    assert_eq!(
        names.len(),
        2,
        "only the scratch worktree was added: {names:?}"
    );
}

#[test]
fn replacing_a_stale_scratch_worktree_removes_only_its_own_entry() {
    let (repo, home, first, second) = fixture();
    let other = tempfile::tempdir().expect("elsewhere");
    let (entry, _away) = invisible_worktree(repo.path(), other.path());
    prepare(repo.path(), home.path(), &first).expect("first");
    // The folder is lost and its entry stays: the stale case.
    fs::remove_dir_all(home.path().join("base")).expect("folder lost");
    let checkout = prepare(repo.path(), home.path(), &second).expect("again");
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), second);
    let found = entries(repo.path());
    assert!(found.iter().any(|(name, _)| *name == entry), "{found:?}");
    let ours: Vec<_> = found
        .iter()
        .filter(|(_, gitdir)| Path::new(gitdir) == checkout.join(".git"))
        .collect();
    assert_eq!(
        ours.len(),
        1,
        "one entry for the scratch worktree: {found:?}"
    );
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_folder_the_repository_forgot_is_replaced_without_touching_the_others() {
    let (repo, home, first, second) = fixture();
    let other = tempfile::tempdir().expect("elsewhere");
    let (entry, _away) = invisible_worktree(repo.path(), other.path());
    let checkout = prepare(repo.path(), home.path(), &first).expect("first");
    // Its entry is removed by hand, the folder stays: the other stale case.
    for (name, gitdir) in entries(repo.path()) {
        if Path::new(&gitdir) == checkout.join(".git") {
            fs::remove_dir_all(repo.path().join(".git/worktrees").join(name)).expect("entry");
        }
    }
    prepare(repo.path(), home.path(), &second).expect("again");
    assert!(entries(repo.path()).iter().any(|(name, _)| *name == entry));
    assert_eq!(text(&checkout), "two");
}

#[test]
fn nothing_is_moved_cleaned_or_removed_outside_the_scratch_worktree() {
    let (repo, home, _, _) = fixture();
    let root = repo.path();
    // Not <home>/base.
    assert!(ours(root, home.path(), &home.path().join("elsewhere")).is_err());
    assert!(
        ours(root, home.path(), root).is_err(),
        "the checkout it runs in"
    );
    assert!(ours(root, home.path(), &home.path().join("base/nested")).is_err());
    // A home that is itself a checkout, or that holds the checkout it runs in.
    assert!(
        ours(home.path(), root, &root.join("base")).is_err(),
        "a home with a .git"
    );
    let holder = tempfile::tempdir().expect("holder");
    let running = holder.path().join("base/work");
    fs::create_dir_all(&running).expect("running");
    assert!(
        ours(&running, holder.path(), &holder.path().join("base")).is_err(),
        "the checkout `make` runs in is inside `<home>/base`"
    );
    // The one place it may touch.
    ours(root, home.path(), &home.path().join("base")).expect("the scratch worktree");
}

#[test]
fn a_directory_that_is_not_ours_survives_a_refusal() {
    let (repo, _home, first, _) = fixture();
    let precious = tempfile::tempdir().expect("precious");
    fs::write(precious.path().join("keep.txt"), "mine").expect("file");
    let wrong_home = precious.path().join(".git");
    fs::create_dir_all(&wrong_home).expect("a checkout-looking home");
    assert!(prepare(repo.path(), precious.path(), &first).is_err());
    assert!(precious.path().join("keep.txt").is_file());
}

/// The source of the scratch worktree, of the flow and of ship's side never asks Git to prune,
/// repair or collect worktrees as a whole: in the container the other worktrees look missing.
#[test]
fn no_code_of_the_game_score_asks_git_to_prune_repair_or_collect_worktrees() {
    fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if path.is_dir() {
                if name != "tests" {
                    sources(&path, found);
                }
            } else if path.extension().is_some_and(|e| e == "rs") && name != "tests.rs" {
                found.push(path);
            }
        }
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    for directory in ["game_score", "ship/game_score", "game_map"] {
        sources(&src.join(directory), &mut files);
    }
    assert!(files.len() > 10, "{files:?}");
    for file in files {
        let text = fs::read_to_string(&file).expect("source");
        for forbidden in ["\"prune\"", "\"repair\"", "\"gc\"", "--prune"] {
            assert!(
                !text.contains(forbidden),
                "{} names {forbidden}: never prune, repair or collect worktrees",
                file.display()
            );
        }
    }
}
