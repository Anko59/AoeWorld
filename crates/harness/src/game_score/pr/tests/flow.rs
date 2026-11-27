//! The flow with a scripted runner: what is played, what is found in the cache, what is stored,
//! and what the outcome says. No build, no game, no repository (except for the two Git helpers).
use super::render::MARKER;
use super::*;
use crate::game_score::pr::fixtures::{MAP, diverging, set, spread, standard};
use serde_json::json;
use std::cell::{Cell, RefCell};

struct Scripted {
    branch: RefCell<Option<Value>>,
    crash: Option<String>,
    base: Result<Value, String>,
    branch_calls: Cell<usize>,
    base_calls: Cell<usize>,
    asked: RefCell<Vec<String>>,
}

impl Scripted {
    fn new(branch: Value, base: Value) -> Self {
        Self {
            branch: RefCell::new(Some(branch)),
            crash: None,
            base: Ok(base),
            branch_calls: Cell::new(0),
            base_calls: Cell::new(0),
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl Runner for Scripted {
    fn play_branch(&self, _: &Run) -> Result<BranchPlayed, BoxError> {
        self.branch_calls.set(self.branch_calls.get() + 1);
        Ok(BranchPlayed {
            report: self.branch.borrow().clone(),
            crash: self.crash.clone(),
            build_seconds: 30.0,
            play_seconds: 70.0,
        })
    }

    fn play_base(&self, _: &Run, scenarios: &[String]) -> Result<BasePlayed, BoxError> {
        self.base_calls.set(self.base_calls.get() + 1);
        *self.asked.borrow_mut() = scenarios.to_vec();
        match &self.base {
            Ok(report) => Ok(BasePlayed {
                report: report.clone(),
                build_seconds: 143.0,
                play_seconds: 61.0,
            }),
            Err(why) => Err(why.clone().into()),
        }
    }
}

fn run_for(home: &Path) -> Run {
    Run {
        root: PathBuf::from("/repo"),
        home: home.to_owned(),
        head: "a".repeat(40),
        base: "b".repeat(40),
        map_hash: MAP.to_owned(),
        package_directory: PathBuf::from("/packages"),
        branch_key: "branch-key".to_owned(),
        base_key: "base-key".to_owned(),
        dirty: false,
        description: String::new(),
    }
}

fn worse() -> Value {
    let mut branch = standard();
    set(
        &mut branch,
        "late",
        "late",
        "service.arrival.arrived_permille",
        json!(900),
    );
    branch
}

fn stored(home: &Path, key: &str) -> Option<cache::Entry> {
    cache::load(&home::results(home), key)
}

#[test]
fn a_miss_plays_both_sides_stores_both_entries_and_compares() {
    let home = tempfile::tempdir().expect("home");
    let runner = Scripted::new(worse(), standard());
    let outcome = execute(&run_for(home.path()), &runner, &spread(1.0)).expect("outcome");
    assert!(outcome.ran);
    assert_eq!((runner.branch_calls.get(), runner.base_calls.get()), (1, 1));
    assert_eq!(*runner.asked.borrow(), ["natural", "late"]);
    assert_eq!(
        stored(home.path(), "branch-key").map(|e| e.revision),
        Some("a".repeat(40))
    );
    let base = stored(home.path(), "base-key").expect("the merge base is kept");
    assert_eq!((base.revision, base.workers), ("b".repeat(40), vec![1, 15]));
    assert_eq!((base.build_seconds, base.play_seconds), (143.0, 61.0));
    assert!(outcome.section.starts_with(MARKER));
    assert!(
        outcome
            .section
            .contains("late/late share of orders arrived")
    );
    assert!(outcome.terminal.contains(
        "merge base: cache miss, built in 143 s, played in 61 s, stored as entry base-key"
    ));
    assert!(
        outcome
            .terminal
            .contains("this branch: built in 30 s, played in 70 s, stored as entry branch-key")
    );
    assert_eq!(outcome.head, "a".repeat(40));
    assert_eq!(outcome.base, Some("b".repeat(40)));
    assert!(
        outcome.summary.contains(
            "different game: 1 numbers moved, 1 beyond their usual spread (1 worse, 0 explained)"
        ),
        "{}",
        outcome.summary
    );
    assert!(
        outcome
            .section
            .contains("**different game**: 1 numbers moved, 1 beyond their usual spread"),
        "the first lines say which case it is"
    );
    assert!(
        outcome.section.contains("<!-- added-to-ship -->"),
        "ship fills in what the check added to it"
    );
}

#[test]
fn the_branch_is_played_at_one_worker_and_four_not_at_every_core() {
    let flags = real::branch_flags(&run_for(Path::new("/h")), Path::new("/o/report.json"));
    assert_eq!(
        flags,
        [
            "--package-directory",
            "/packages",
            "--map-hash",
            MAP,
            "--workers",
            "1,4",
            "--parallel",
            "--out",
            "/o/report.json"
        ]
    );
    assert_eq!(real::WORKERS, "1,4");
}

#[test]
fn the_same_game_is_said_in_the_summary_and_in_the_first_line() {
    let home = tempfile::tempdir().expect("home");
    let runner = Scripted::new(standard(), standard());
    let outcome = execute(&run_for(home.path()), &runner, &spread(1.0)).expect("outcome");
    assert!(
        outcome
            .summary
            .starts_with("informational: same game as the merge base ("),
        "{}",
        outcome.summary
    );
    assert!(
        outcome
            .section
            .contains("- **same game**: every number of every compared window")
    );
}

#[test]
fn a_hit_for_the_merge_base_plays_only_the_branch() {
    let home = tempfile::tempdir().expect("home");
    let first = Scripted::new(standard(), standard());
    execute(&run_for(home.path()), &first, &spread(1.0)).expect("first");
    // Another commit of the branch (new game code), the same merge base.
    let mut next = run_for(home.path());
    next.branch_key = "other-branch-key".to_owned();
    let second = Scripted::new(worse(), standard());
    let outcome = execute(&next, &second, &spread(1.0)).expect("second");
    assert_eq!((second.branch_calls.get(), second.base_calls.get()), (1, 0));
    assert!(
        outcome.terminal.contains("merge base: cache hit in "),
        "{}",
        outcome.terminal
    );
    assert!(
        outcome
            .terminal
            .contains("(entry base-key, made from bbbbbbbbbbbb ")
    );
    assert!(
        outcome
            .section
            .contains("late/late share of orders arrived")
    );
}

#[test]
fn a_game_state_already_played_is_not_played_again() {
    let home = tempfile::tempdir().expect("home");
    let run = run_for(home.path());
    let first = Scripted::new(worse(), standard());
    let a = execute(&run, &first, &spread(1.0)).expect("first");
    let second = Scripted::new(standard(), standard());
    let b = execute(&run, &second, &spread(1.0)).expect("second");
    assert_eq!((second.branch_calls.get(), second.base_calls.get()), (0, 0));
    assert!(
        b.terminal.contains("this branch: cache hit in "),
        "{}",
        b.terminal
    );
    assert_eq!(a.summary, b.summary, "the same numbers, said the same way");
}

#[test]
fn a_working_tree_that_differs_from_head_is_played_every_time_and_not_stored() {
    let home = tempfile::tempdir().expect("home");
    let mut run = run_for(home.path());
    run.dirty = true;
    let runner = Scripted::new(worse(), standard());
    let outcome = execute(&run, &runner, &spread(1.0)).expect("first");
    let again = Scripted::new(worse(), standard());
    execute(&run, &again, &spread(1.0)).expect("second");
    assert_eq!(again.branch_calls.get(), 1, "never a hit");
    assert!(stored(home.path(), "branch-key").is_none());
    assert!(
        stored(home.path(), "base-key").is_some(),
        "the merge base is still the merge base"
    );
    assert!(outcome.section.contains(
        "note: the working tree differs from HEAD in the game's inputs: this result is not stored"
    ));
}

#[test]
fn runs_that_differ_between_worker_counts_are_reported_and_never_stored() {
    let home = tempfile::tempdir().expect("home");
    let runner = Scripted::new(diverging(standard()), standard());
    let outcome = execute(&run_for(home.path()), &runner, &spread(1.0)).expect("outcome");
    assert!(outcome.ran);
    assert!(
        outcome
            .section
            .contains("determinism: natural DIFFERENT GAMES"),
        "{}",
        outcome.section
    );
    assert!(stored(home.path(), "branch-key").is_none());
    assert!(outcome.summary.contains("different games"));
}

#[test]
fn a_branch_that_crashed_is_a_verdict_and_the_merge_base_is_left_alone() {
    let home = tempfile::tempdir().expect("home");
    let mut runner = Scripted::new(Value::Null, standard());
    runner.branch = RefCell::new(None);
    runner.crash = Some("the release build of this branch failed: boom".to_owned());
    let outcome = execute(&run_for(home.path()), &runner, &spread(1.0)).expect("outcome");
    assert!(outcome.ran);
    assert_eq!(runner.base_calls.get(), 0);
    assert!(
        outcome
            .section
            .contains("crash: the release build of this branch failed: boom")
    );
    assert!(outcome.summary.contains("crashed"), "{}", outcome.summary);
    assert!(stored(home.path(), "branch-key").is_none());
}

#[test]
fn a_merge_base_that_cannot_be_played_leaves_the_branchs_numbers_and_one_line() {
    let home = tempfile::tempdir().expect("home");
    let mut runner = Scripted::new(worse(), standard());
    runner.base = Err("its release build failed: error[E0432]".to_owned());
    let outcome = execute(&run_for(home.path()), &runner, &spread(1.0)).expect("outcome");
    assert!(outcome.ran);
    assert!(
        outcome.terminal.contains("merge base: not available (the merge base bbbbbbbbbbbb could not be played: its release build failed: error[E0432])"),
        "{}",
        outcome.terminal
    );
    assert!(
        outcome
            .section
            .contains("Not compared (no numbers for the merge base)")
    );
    assert!(
        outcome
            .summary
            .starts_with("informational: branch numbers only"),
        "{}",
        outcome.summary
    );
    assert!(
        stored(home.path(), "base-key").is_none(),
        "nothing to remember"
    );
    assert!(
        stored(home.path(), "branch-key").is_some(),
        "the branch's numbers still are"
    );
}

#[test]
fn the_description_explains_a_move_that_was_intended() {
    let home = tempfile::tempdir().expect("home");
    let mut run = run_for(home.path());
    run.description =
        "## Why\nbecause\n\ngame-score: late arrived -7% because the army now waits\n".to_owned();
    let outcome =
        execute(&run, &Scripted::new(worse(), standard()), &spread(1.0)).expect("outcome");
    assert!(
        outcome
            .section
            .contains("worse, explained: the army now waits"),
        "{}",
        outcome.section
    );
    assert!(
        outcome.summary.contains("(1 worse, 1 explained)"),
        "{}",
        outcome.summary
    );
}

#[test]
fn a_skipped_run_says_why_in_one_line_and_holds_no_section() {
    let repository = tempfile::tempdir().expect("repository");
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "fixture@example.invalid"],
        &["config", "user.name", "Fixture"],
        &["config", "commit.gpgsign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "one"],
    ] {
        git(repository.path(), args).expect("git");
    }
    let reason = format!(
        "skipped because {}",
        "nothing the game can feel changed ".repeat(10)
    );
    let outcome = skipped(repository.path(), &reason).expect("outcome");
    assert!(!outcome.ran);
    assert!(outcome.section.is_empty() && outcome.terminal.is_empty());
    assert_eq!(outcome.summary.chars().count(), 140);
    assert_eq!(outcome.head.len(), 40);
    let text = serde_json::to_string(&outcome).expect("json");
    let back: Outcome = serde_json::from_str(&text).expect("back");
    assert_eq!((back.ran, back.head, back.schema), (false, outcome.head, 1));
}

#[test]
fn the_merge_base_is_found_against_a_branch_of_origin() {
    let repository = tempfile::tempdir().expect("repository");
    let root = repository.path();
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "fixture@example.invalid"],
        &["config", "user.name", "Fixture"],
        &["config", "commit.gpgsign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "dev"],
    ] {
        git(root, args).expect("git");
    }
    let dev = git(root, &["rev-parse", "HEAD"]).expect("dev");
    git(root, &["update-ref", "refs/remotes/origin/dev", &dev]).expect("ref");
    git(root, &["commit", "-q", "--allow-empty", "-m", "branch"]).expect("branch");
    assert_eq!(merge_base(root, "origin/dev").expect("base"), dev);
    git(root, &["tag", "origin/dev"]).expect("a tag that looks like the ref");
    assert_eq!(
        merge_base(root, "origin/dev").expect("base"),
        dev,
        "a tag cannot shadow it"
    );
    assert!(merge_base(root, "origin/missing").is_err());
}

#[test]
fn the_reports_of_the_scenarios_played_apart_become_one() {
    let natural = crate::game_score::pr::fixtures::report(vec![standard()["scenarios"][0].clone()]);
    let late = crate::game_score::pr::fixtures::report(vec![standard()["scenarios"][1].clone()]);
    let merged = real::merge(vec![natural, late]);
    assert_eq!(scenario_ids(&merged), ["natural", "late"]);
    assert_eq!(merged["header"]["map_hash"], MAP);
    assert_eq!(merged["schema"], 1);
    assert_eq!(worker_counts(&merged), [1, 15]);
}
