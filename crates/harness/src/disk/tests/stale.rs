use crate::disk::stale::{Class, Row, clean, hint, merged_head};
use crate::ship::git::git;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

const HOUR: Duration = Duration::from_secs(3600);

/// A main checkout on `dev`, mirrored as `origin/dev`, and named worktrees beside it.
struct Fixture {
    temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let main = temp.path().join("main");
        fs::create_dir(&main).unwrap();
        for args in [
            &["init", "-b", "dev"][..],
            &["config", "user.name", "Stale fixture"],
            &["config", "user.email", "stale@example.invalid"],
            &["commit", "--allow-empty", "-m", "base"],
            &["update-ref", "refs/remotes/origin/dev", "HEAD"],
        ] {
            git(&main, args).unwrap();
        }
        fs::write(main.join(".gitignore"), "target/\n").unwrap();
        git(&main, &["add", ".gitignore"]).unwrap();
        git(&main, &["commit", "-m", "ignore"]).unwrap();
        git(&main, &["update-ref", "refs/remotes/origin/dev", "HEAD"]).unwrap();
        Self { temp }
    }

    fn main(&self) -> PathBuf {
        self.temp.path().join("main")
    }

    /// A worktree on `branch` with a `target/` file; `commit` adds an unmerged commit.
    fn add(&self, branch: &str, commit: bool) -> PathBuf {
        let path = self.temp.path().join(branch);
        let path_text = path.to_str().unwrap();
        git(
            &self.main(),
            &["worktree", "add", "-b", branch, path_text, "dev"],
        )
        .unwrap();
        if commit {
            git(&path, &["commit", "--allow-empty", "-m", branch]).unwrap();
        }
        fs::create_dir_all(path.join("target/debug")).unwrap();
        fs::write(path.join("target/debug/object"), "build output").unwrap();
        path
    }

    fn run(&self, cwd: &Path, apply: bool, later: Duration) -> Vec<Row> {
        let squashed = |_: &Path, branch: &str, _: &str| branch == "squashed";
        clean(cwd, apply, SystemTime::now() + later, &squashed).unwrap()
    }
}

fn outcome<'a>(rows: &'a [Row], path: &Path) -> &'a Row {
    rows.iter().find(|row| row.path == path).unwrap()
}

#[test]
fn merged_clean_worktree_is_removed_only_under_apply() {
    let fixture = Fixture::new();
    let merged = fixture.add("merged", false);
    let squashed = fixture.add("squashed", true);
    let rows = fixture.run(&fixture.main(), false, 2 * HOUR);
    for path in [&merged, &squashed] {
        let row = outcome(&rows, path);
        assert_eq!(
            (row.class, row.outcome.as_str()),
            (Class::Merged, "would remove worktree")
        );
        assert!(row.freed > 0 && path.is_dir());
    }
    // Within the grace hour a merged worktree may be one an agent just created.
    let fresh = fixture.run(&fixture.main(), true, Duration::ZERO);
    assert_eq!(
        outcome(&fresh, &merged).outcome,
        "kept: modified in the last hour"
    );
    let rows = fixture.run(&fixture.main(), true, 2 * HOUR);
    for path in [&merged, &squashed] {
        assert_eq!(outcome(&rows, path).outcome, "removed worktree");
        assert!(!path.exists());
    }
}

#[test]
fn uncommitted_change_keeps_a_merged_worktree() {
    let fixture = Fixture::new();
    let dirty = fixture.add("dirty", false);
    fs::write(dirty.join("notes.txt"), "untracked").unwrap();
    let rows = fixture.run(&fixture.main(), true, 4 * 24 * HOUR);
    assert_eq!(outcome(&rows, &dirty).outcome, "kept: uncommitted changes");
    assert!(dirty.join("target/debug/object").is_file());
}

#[test]
fn idle_worktree_loses_only_its_target() {
    let fixture = Fixture::new();
    let feature = fixture.add("feature", true);
    let active = fixture.run(&fixture.main(), true, 2 * HOUR);
    assert_eq!(outcome(&active, &feature).class, Class::Active);
    assert!(feature.join("target").is_dir());
    let rows = fixture.run(&fixture.main(), true, 4 * 24 * HOUR);
    let row = outcome(&rows, &feature);
    assert_eq!(
        (row.class, row.outcome.as_str()),
        (Class::Idle, "removed target/")
    );
    assert_eq!(row.freed, "build output".len() as u64);
    assert!(!feature.join("target").exists());
    assert!(feature.join(".gitignore").is_file());
}

#[test]
fn main_checkout_and_current_worktree_are_never_touched() {
    let fixture = Fixture::new();
    let current = fixture.add("current", false);
    fs::create_dir_all(fixture.main().join("target")).unwrap();
    let rows = fixture.run(&current, true, 4 * 24 * HOUR);
    assert_eq!(
        outcome(&rows, &fixture.main()).outcome,
        "kept: main checkout"
    );
    assert_eq!(outcome(&rows, &current).outcome, "kept: current worktree");
    assert!(fixture.main().join("target").is_dir());
    assert!(current.join("target/debug/object").is_file());
}

#[test]
fn worktree_used_as_a_working_directory_is_kept() {
    let fixture = Fixture::new();
    let busy = fixture.add("busy", false);
    let mut sleep = Command::new("sleep")
        .arg("60")
        .current_dir(busy.join("target"))
        .spawn()
        .unwrap();
    let rows = fixture.run(&fixture.main(), true, 4 * 24 * HOUR);
    sleep.kill().unwrap();
    sleep.wait().unwrap();
    assert_eq!(outcome(&rows, &busy).outcome, "kept: in use");
    assert!(busy.join("target/debug/object").is_file());
}

#[test]
fn squash_merge_needs_a_merged_pull_request_with_this_head() {
    let json = r#"[{"number":7,"headRefOid":"abc"}]"#;
    assert!(merged_head(json, "abc"));
    assert!(!merged_head(json, "def"));
    assert!(!merged_head("[]", "abc"));
    assert!(!merged_head("not json", "abc"));
}

#[test]
fn ship_hint_appears_only_below_fifty_gib() {
    let gib = 1024 * 1024 * 1024;
    assert_eq!(
        hint(12 * gib).unwrap(),
        "disk: 12 GiB free; run `make clean-stale` (dry run) / `make clean-stale APPLY=1`"
    );
    assert!(hint(50 * gib).is_none());
}
