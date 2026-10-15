use super::*;
use crate::{
    review::Tier,
    ship::{
        evidence, judge,
        review_gate::graded,
        ship, ship_with,
        tests::{fixture as plain_fixture, offline, run},
    },
};
use std::{fs, path::PathBuf};

mod pinned;
mod restack;

type Temp = tempfile::TempDir;

/// The ship fixture, with the committer identity a rebase needs.
fn fixture(gate: &str) -> (Temp, PathBuf) {
    let (temp, root) = plain_fixture(gate);
    run(&root, &["config", "user.name", "t"]);
    run(&root, &["config", "user.email", "t@example.com"]);
    (temp, root)
}

fn git(root: &Path, args: &[&str]) -> String {
    git::git(root, args).unwrap()
}

fn origin(temp: &Temp) -> PathBuf {
    temp.path().join("origin.git")
}

/// Another checkout moves `branch` on origin by one commit adding `file`.
fn advance(temp: &Temp, branch: &str, file: &str, text: &str) -> String {
    let other = temp.path().join("other");
    if !other.exists() {
        run(
            temp.path(),
            &[
                "clone",
                "-q",
                origin(temp).to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
    }
    run(&other, &["fetch", "-q", "origin"]);
    run(
        &other,
        &["checkout", "-q", "-B", branch, &format!("origin/{branch}")],
    );
    fs::create_dir_all(other.join(file).parent().unwrap()).unwrap();
    fs::write(other.join(file), text).unwrap();
    run(&other, &["add", "-A"]);
    run(&other, &["commit", "-q", "-m", &format!("move {file}")]);
    run(&other, &["push", "-q", "origin", branch]);
    git(&other, &["rev-parse", "HEAD"])
}

fn remote_tip(root: &Path, branch: &str) -> String {
    git(
        root,
        &["ls-remote", "origin", &format!("refs/heads/{branch}")],
    )
    .split_whitespace()
    .next()
    .unwrap_or_default()
    .to_owned()
}

fn head(root: &Path) -> String {
    git(root, &["rev-parse", "HEAD"])
}

fn contains(root: &Path, ancestor: &str) -> bool {
    git::git(root, &["merge-base", "--is-ancestor", ancestor, "HEAD"]).is_ok()
}

fn runs(temp: &Temp) -> usize {
    fs::read_to_string(temp.path().join("gate.log")).map_or(0, |log| log.lines().count())
}

const LOGGED: &str = "echo run >> ../gate.log";
/// The real case: a directory limited to one file.
const ONE_FILE_DIR: &str = "[ $$(ls dir 2>/dev/null | wc -l) -le 1 ]";

#[test]
fn an_unmoved_base_is_not_rechecked() {
    let (temp, root) = fixture(LOGGED);
    let before = judge(&root, &offline()).unwrap();
    assert!(
        before_push(&root, &before, None, &offline())
            .unwrap()
            .is_none()
    );
    assert_eq!(runs(&temp), 1, "only the ship's own gate run");
    assert_eq!(head(&root), before.head);
    let shipped = ship(&root, &offline()).unwrap();
    assert!(shipped.recheck.is_none());
    assert_eq!(remote_tip(&root, "feature"), shipped.head);
}

#[test]
fn a_moved_base_is_rebased_rechecked_pushed_and_recorded() {
    let (temp, root) = fixture(LOGGED);
    let moved = advance(&temp, "dev", "other.txt", "o\n");
    let shipped = ship(&root, &offline()).unwrap();
    assert_eq!(runs(&temp), 2, "the ship's gates, then the static re-check");
    let recheck = shipped.recheck.as_ref().expect("recheck recorded");
    assert_eq!(
        (recheck.rounds, recheck.commits.clone()),
        (1, vec![moved.clone()])
    );
    assert_eq!(
        shipped.gates[0].gate, "fmt-check",
        "the static gates ran on the rebased head"
    );
    assert_eq!(shipped.cadence, "recheck", "not a copied preflight verdict");
    assert_eq!(recheck.head, shipped.head);
    assert_ne!(recheck.from_head, shipped.head);
    assert_eq!(
        recheck.preflight[0].gate, "fmt-check",
        "the pre-rebase preflight is a reference"
    );
    assert_eq!(
        (shipped.base.as_str(), recheck.base.as_str()),
        (moved.as_str(), moved.as_str())
    );
    assert_eq!(shipped.head, head(&root));
    assert!(contains(&root, &moved));
    assert_eq!(shipped.changed, ["README.md"], "only the own change");
    assert_eq!(remote_tip(&root, "feature"), shipped.head, "pushed");
    let stored = evidence::read(&root, &shipped.head).unwrap().unwrap();
    assert_eq!(stored.recheck.unwrap().from_head, recheck.from_head);
    let text = section(&shipped);
    assert!(
        text.contains("Re-check before push")
            && text.contains("fmt-check")
            && text.contains(&format!("Full preflight on `{}`", &recheck.from_head[..12])),
        "{text}"
    );
    assert_eq!(section(&judge(&root, &offline()).unwrap()), "");
}

#[test]
fn a_combination_that_breaks_a_static_gate_stops_the_ship_before_the_push() {
    let (temp, root) = fixture(ONE_FILE_DIR);
    fs::create_dir_all(root.join("dir")).unwrap();
    fs::write(root.join("dir/mine"), "m\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "mine"]);
    let moved = advance(&temp, "dev", "dir/theirs", "t\n");
    let error = ship(&root, &offline()).unwrap_err().to_string();
    assert!(
        error.contains("re-check FAILED: fmt-check failed"),
        "{error}"
    );
    assert!(
        error.contains(&format!(
            "combination with 1 new commit(s) on dev ({})",
            &moved[..12]
        )),
        "{error}"
    );
    assert!(error.contains("Nothing was pushed"), "{error}");
    assert_eq!(remote_tip(&root, "feature"), "");
    assert!(
        root.join("dir/theirs").exists(),
        "the rebased branch stays for the fix"
    );
    assert!(contains(&root, &moved));
}

#[test]
fn a_conflict_aborts_the_rebase_and_names_the_files() {
    let (temp, root) = fixture("true");
    let before = head(&root);
    advance(&temp, "dev", "README.md", "z\n");
    let error = ship(&root, &offline()).unwrap_err().to_string();
    assert!(
        error.contains("dev moved during the ship and conflicts in README.md"),
        "{error}"
    );
    assert!(error.contains("rebase, resolve, ship again"), "{error}");
    assert!(
        error.contains("only the changed part is reviewed"),
        "{error}"
    );
    assert_eq!(head(&root), before);
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    let rebasing = git(&root, &["rev-parse", "--git-path", "rebase-merge"]);
    assert!(!root.join(rebasing).exists());
    assert_eq!(remote_tip(&root, "feature"), "");
}

#[test]
fn a_base_that_keeps_moving_stops_after_three_rechecks() {
    let (temp, root) = fixture("true");
    let mover = temp.path().join("mover.sh");
    fs::write(
        &mover,
        "set -e\nunset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE\ncd \"$2\"\n\
         git fetch -q origin\ngit checkout -q -B dev origin/dev\n\
         n=$(git rev-list --count HEAD)\necho \"$n\" > \"moved$n.txt\"\ngit add .\n\
         git -c user.name=t -c user.email=t@e.com commit -qm \"moved$n\"\ngit push -q origin dev\n",
    )
    .unwrap();
    let other = temp.path().join("other");
    run(
        temp.path(),
        &[
            "clone",
            "-q",
            origin(&temp).to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    let gate = format!("sh {} x {}", mover.display(), other.display());
    fs::write(root.join("Makefile"), format!("fmt-check:\n\t@{gate}\n")).unwrap();
    run(&root, &["commit", "-q", "-am", "moving gate"]);
    let error = ship(&root, &offline()).unwrap_err().to_string();
    assert!(error.contains("kept moving"), "{error}");
    assert!(error.contains("3 re-checks"), "{error}");
    assert_eq!(remote_tip(&root, "feature"), "");
    assert_eq!(MAX_ROUNDS, 3);
}

#[test]
fn a_stacked_ship_rechecks_against_its_parent_not_dev() {
    let (temp, root) = fixture(LOGGED);
    run(&root, &["checkout", "-q", "-b", "parent", "dev"]);
    fs::write(root.join("p.txt"), "p\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "parent"]);
    run(&root, &["push", "-q", "origin", "parent"]);
    run(&root, &["checkout", "-q", "-B", "feature", "parent"]);
    fs::write(root.join("c.txt"), "c\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "child"]);
    let options = Options {
        base: "parent".into(),
        ..offline()
    };
    let before = judge(&root, &options).unwrap();
    assert_eq!(before.changed, ["c.txt"]);
    advance(&temp, "dev", "dev-only.txt", "d\n");
    assert!(
        before_push(&root, &before, None, &options)
            .unwrap()
            .is_none(),
        "dev is not the base"
    );
    let moved = advance(&temp, "parent", "p2.txt", "p2\n");
    let rebased = before_push(&root, &before, None, &options)
        .unwrap()
        .expect("parent moved");
    assert_eq!(rebased.evidence.base, moved);
    assert_eq!(rebased.evidence.base_branch, "parent");
    assert_eq!(rebased.evidence.changed, ["c.txt"]);
    assert!(contains(&root, &moved));
    assert_eq!(
        git(&root, &["rev-list", "--count", &format!("{moved}..HEAD")]),
        "1"
    );
    assert!(!root.join("dev-only.txt").exists());
}

#[test]
fn only_the_ships_own_remote_tip_is_overwritten_without_ship_force() {
    // The remote branch is at the commit the ship started from: leased.
    let (temp, root) = fixture("true");
    run(&root, &["push", "-q", "origin", "feature"]);
    let before = head(&root);
    advance(&temp, "dev", "other.txt", "o\n");
    let shipped = ship(&root, &offline()).unwrap();
    assert_ne!(shipped.head, before);
    assert_eq!(
        remote_tip(&root, "feature"),
        shipped.head,
        "leased push replaced the tip"
    );
    // Another tip, no SHIP_FORCE: stop and ask for it.
    let (temp, root) = fixture("true");
    run(&root, &["push", "-q", "origin", "feature"]);
    run(&root, &["commit", "-q", "--amend", "-m", "amended"]);
    let tip = remote_tip(&root, "feature");
    let before = judge(&root, &offline()).unwrap();
    advance(&temp, "dev", "other.txt", "o\n");
    let error = before_push(&root, &before, None, &offline())
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("SHIP_FORCE=1") && error.contains(&tip[..12]),
        "{error}"
    );
    assert_eq!(remote_tip(&root, "feature"), tip);
    // With SHIP_FORCE the plain `--force-with-lease` of today applies.
    let (temp, root) = fixture("true");
    run(&root, &["push", "-q", "origin", "feature"]);
    run(&root, &["commit", "-q", "--amend", "-m", "amended"]);
    let before = judge(&root, &offline()).unwrap();
    advance(&temp, "dev", "other.txt", "o\n");
    let forced = Options {
        force: true,
        ..offline()
    };
    let rebased = before_push(&root, &before, None, &forced).unwrap().unwrap();
    assert_eq!(rebased.lease, None);
    drop(rebased); // releases the pinned base of this fixture
    // A branch not on the remote needs no force at all.
    let (temp, root) = fixture("true");
    let before = judge(&root, &offline()).unwrap();
    advance(&temp, "dev", "other.txt", "o\n");
    assert_eq!(
        before_push(&root, &before, None, &offline())
            .unwrap()
            .unwrap()
            .lease,
        None
    );
}

#[test]
fn the_passed_review_carries_over_a_clean_rebase_without_running_again() {
    let (temp, root) = fixture("true");
    graded(&root, Tier::Low, 9);
    let options = Options {
        no_review: false,
        ..offline()
    };
    advance(&temp, "dev", "other.txt", "o\n");
    let rejects =
        |_: &Path, _: Tier, _: crate::agents::Runtime, _: &str, _: &crate::review::Plan| {
            Err::<crate::review::Report, _>("no review may run".into())
        };
    let shipped = ship_with(&root, &options, &rejects).unwrap();
    assert!(shipped.recheck.is_some());
    assert_eq!(remote_tip(&root, "feature"), shipped.head);
}

#[test]
fn a_gate_over_its_budget_states_the_load() {
    let busy = || {
        Some(crate::process::load::Load {
            one: 40.0,
            five: 30.0,
            fifteen: 20.0,
            cores: 12,
        })
    };
    let line = crate::ship::run::over_budget(600, "make: *** killed", &busy);
    assert!(
        line.starts_with("over its 600s budget (machine load was 40.0 / 30.0 / 20.0"),
        "{line}"
    );
    assert!(
        line.contains("probably not your change") && line.ends_with("make: *** killed"),
        "{line}"
    );
    assert_eq!(
        crate::ship::run::over_budget(600, "x", &|| None),
        "over its 600s budget; x"
    );
}
