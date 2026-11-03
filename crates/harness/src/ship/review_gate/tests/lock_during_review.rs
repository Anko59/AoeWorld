//! The ship lock is let go while the review runs and taken again for the push (#388).
use super::{graded, reviewed};
use crate::ship::{
    Options, git,
    lock::{self, Handoff, Holder},
    ship_with,
    tests::{fixture, offline, run},
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(20);

fn holder(root: &Path, branch: &str) -> Holder {
    Holder {
        worktree: root.display().to_string(),
        branch: branch.into(),
        command: "make ship".into(),
        started: 1_791_900_000,
    }
}

/// The lock as `make ship` takes it, held by a `Handoff`.
fn take(root: &Path) -> Handoff {
    Handoff::holding(
        lock::acquire_at(&lock::path(root).unwrap(), &holder(root, "test")).expect("the lock"),
    )
}

fn fixture_with_identity() -> (tempfile::TempDir, PathBuf) {
    let (temp, root) = fixture("true");
    run(&root, &["config", "user.name", "t"]);
    run(&root, &["config", "user.email", "t@example.com"]);
    (temp, root)
}

fn remote_tip(root: &Path, branch: &str) -> String {
    let line = git::git(
        root,
        &["ls-remote", "origin", &format!("refs/heads/{branch}")],
    )
    .unwrap();
    line.split_whitespace().next().unwrap_or("").to_owned()
}

fn with_lock(root: &Path, options: Options) -> Options {
    Options {
        lock: take(root),
        ..options
    }
}

#[test]
fn the_next_ship_runs_its_gates_while_the_first_reviews_and_both_push() {
    let (temp, root) = fixture_with_identity();
    let second = temp.path().join("second");
    run(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature2",
            second.to_str().unwrap(),
            "feature",
        ],
    );
    std::fs::write(second.join("two.txt"), "2\n").unwrap();
    run(&second, &["add", "-A"]);
    run(&second, &["commit", "-q", "-m", "second"]);
    let (reviewing, in_review) = mpsc::channel();
    let (finish, finished) = mpsc::channel::<()>();
    thread::scope(|scope| {
        let work = &root;
        let first = scope.spawn(move || {
            let options = with_lock(work, reviewed());
            ship_with(work, &options, &|root, tier, _, _, _| {
                reviewing.send(()).unwrap();
                finished
                    .recv_timeout(WAIT)
                    .expect("the test releases the review");
                Ok(graded(root, tier, 9))
            })
            .map(|evidence| evidence.head)
            .map_err(|error| error.to_string())
        });
        in_review
            .recv_timeout(WAIT)
            .expect("the first ship reviews");
        // Its lock is free: the second ship takes it at once (no wait), runs its gates and pushes.
        let options = with_lock(&second, offline());
        let pushed = ship_with(&second, &options, &|_, _, _, _, _| Err("no review".into()))
            .expect("the second ship, while the first reviews");
        assert_eq!(remote_tip(&root, "feature2"), pushed.head);
        assert!(
            remote_tip(&root, "feature").is_empty(),
            "the first has not pushed"
        );
        drop(options);
        finish.send(()).unwrap();
        let head = first.join().unwrap().expect("the first ship");
        assert_eq!(remote_tip(&root, "feature"), head);
    });
}

#[test]
fn a_busy_push_stops_and_a_retry_reuses_the_review() {
    let (_temp, root) = fixture_with_identity();
    let (reviewing, in_review) = mpsc::channel();
    let (finish, finished) = mpsc::channel::<()>();
    let (completed, result) = mpsc::channel();
    thread::scope(|scope| {
        let work = &root;
        let first = scope.spawn(move || {
            let options = with_lock(work, reviewed());
            let result = ship_with(work, &options, &|root, tier, _, _, _| {
                reviewing.send(()).unwrap();
                finished.recv_timeout(WAIT).expect("release the review");
                Ok(graded(root, tier, 9))
            })
            .map(|e| e.head)
            .map_err(|e| e.to_string());
            completed.send(result.clone()).unwrap();
            result
        });
        in_review.recv_timeout(WAIT).expect("review started");
        let other = take(&root);
        finish.send(()).unwrap();
        let error = result
            .recv_timeout(Duration::from_secs(5))
            .expect("refusal never waits for the other ship")
            .unwrap_err();
        assert!(error.contains("busy:"), "{error}");
        assert!(remote_tip(&root, "feature").is_empty());
        drop(other);
        first.join().unwrap().unwrap_err();
        let options = with_lock(&root, reviewed());
        let retried = ship_with(&root, &options, &|_, _, _, _, _| {
            panic!("the exact-commit review must be cached")
        })
        .expect("retry reuses the finished review");
        assert_eq!(remote_tip(&root, "feature"), retried.head);
    });
}

#[test]
fn ownlease_busy_ship_stops_before_another_ship_advances_the_checkout() {
    for force in [true, false] {
        let (_temp, root) = fixture_with_identity();
        let h0 = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
        run(&root, &["push", "-q", "origin", "feature"]);
        let (reviewing, in_review) = mpsc::channel();
        let (finish, finished) = mpsc::channel::<()>();
        let (completed, result) = mpsc::channel();
        thread::scope(|scope| {
            let work = &root;
            let first = scope.spawn(move || {
                let options = with_lock(
                    work,
                    Options {
                        force,
                        ..reviewed()
                    },
                );
                let result = ship_with(work, &options, &|root, tier, _, _, _| {
                    let report = graded(root, tier, 9);
                    reviewing.send(report.head.clone()).unwrap();
                    finished.recv_timeout(WAIT).expect("release the review");
                    Ok(report)
                })
                .map(|e| e.head)
                .map_err(|e| e.to_string());
                completed.send(result.clone()).unwrap();
                result
            });
            assert_eq!(in_review.recv_timeout(WAIT).unwrap(), h0);
            let other = take(&root);
            finish.send(()).unwrap();
            let error = result
                .recv_timeout(Duration::from_secs(5))
                .expect("busy ship exits before the holder releases")
                .unwrap_err();
            assert!(error.contains("busy:"), "{error}");
            first.join().unwrap().unwrap_err();
            std::fs::write(root.join("newer.txt"), "newer ship\n").unwrap();
            run(&root, &["add", "newer.txt"]);
            run(&root, &["commit", "-q", "-m", "newer ship"]);
            let newer = ship_with(&root, &offline(), &|_, _, _, _, _| panic!("offline")).unwrap();
            assert_ne!(newer.head, h0);
            drop(other);
            assert_eq!(remote_tip(&root, "feature"), newer.head);
            assert_eq!(git::git(&root, &["rev-parse", "HEAD"]).unwrap(), newer.head);
        });
    }
}

#[test]
fn a_failed_review_leaves_no_lock_and_takes_none_it_did_not_hold() {
    let (_temp, root) = fixture_with_identity();
    let path = lock::path(&root).unwrap();
    let fails = |_: &Path,
                 _: crate::review::Tier,
                 _: crate::agents::Runtime,
                 _: &str,
                 _: &crate::review::Plan| {
        Err::<crate::review::Report, _>("review failed".into())
    };
    let options = with_lock(&root, reviewed());
    let error = ship_with(&root, &options, &fails).unwrap_err().to_string();
    assert!(error.contains("review failed"), "{error}");
    assert!(
        lock::status(&path).unwrap().is_none(),
        "released, not retaken"
    );
    // A ship that holds nothing (review-pr, tests) releases nothing of someone else's.
    let other = take(&root);
    let error = ship_with(&root, &reviewed(), &fails)
        .unwrap_err()
        .to_string();
    assert!(error.contains("review failed"), "{error}");
    assert!(
        lock::status(&path).unwrap().is_some(),
        "the other holder keeps it"
    );
    drop(other);
}

#[test]
fn the_recheck_runs_after_reacquiring_when_dev_moved_during_the_review() {
    let (temp, root) = fixture_with_identity();
    let path = lock::path(&root).unwrap();
    let options = with_lock(&root, reviewed());
    let shipped = ship_with(&root, &options, &|root, tier, _, _, _| {
        let other = temp.path().join("other");
        run(
            temp.path(),
            &[
                "clone",
                "-q",
                temp.path().join("origin.git").to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        );
        run(&other, &["config", "user.name", "t"]);
        run(&other, &["config", "user.email", "t@example.com"]);
        std::fs::write(other.join("moved.txt"), "m\n").unwrap();
        run(&other, &["add", "-A"]);
        run(&other, &["commit", "-q", "-m", "dev moved"]);
        run(&other, &["push", "-q", "origin", "dev"]);
        assert!(
            lock::status(&path).unwrap().is_none(),
            "unlocked during the review"
        );
        Ok(graded(root, tier, 9))
    })
    .expect("ship");
    assert!(shipped.recheck.is_some(), "rebased and re-checked");
    let text = std::fs::read_to_string(root.join(".git/aoe-ship/timeline/feature.jsonl")).unwrap();
    let entries: Vec<serde_json::Value> = text
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let recheck = entries
        .iter()
        .position(|e| e["stage"] == "recheck" && e["event"] == "start")
        .unwrap();
    let end = entries
        .iter()
        .position(|e| e["stage"] == "recheck" && e["event"] == "end")
        .unwrap();
    let gates: Vec<_> = entries[recheck + 1..end]
        .iter()
        .filter(|e| e["stage"] == "gate/fmt-check")
        .collect();
    assert_eq!(
        gates.len(),
        2,
        "recheck observes its own gate attempt: {text}"
    );
    assert_eq!(gates[0]["event"], "start");
    assert_eq!(gates[1]["event"], "end");
    assert_eq!(gates[1]["detail"]["ok"], true);
    assert_eq!(remote_tip(&root, "feature"), shipped.head);
    assert!(
        lock::status(&path).unwrap().is_some(),
        "held again for the push"
    );
    drop(options);
    assert!(lock::status(&path).unwrap().is_none());
}

#[test]
fn the_next_ship_runs_while_the_first_runs_its_gates() {
    // The first ship's gate blocks until the test says go; the second's runs straight through
    // (only the first to create `first` waits).
    let flags = tempfile::tempdir().unwrap();
    let dir = flags.path().display();
    let gate = format!(
        "if mkdir {dir}/first 2>/dev/null; then touch {dir}/started; \
         while [ ! -f {dir}/go ]; do sleep 0.05; done; fi"
    );
    let (temp, root) = fixture(&gate);
    run(&root, &["config", "user.name", "t"]);
    run(&root, &["config", "user.email", "t@example.com"]);
    let second = temp.path().join("second");
    run(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature2",
            second.to_str().unwrap(),
            "feature",
        ],
    );
    std::fs::write(second.join("two.txt"), "2\n").unwrap();
    run(&second, &["add", "-A"]);
    run(&second, &["commit", "-q", "-m", "second"]);
    thread::scope(|scope| {
        let work = &root;
        let first = scope.spawn(move || {
            let options = with_lock(work, offline());
            ship_with(work, &options, &|_, _, _, _, _| Err("no review".into()))
                .map(|evidence| evidence.head)
                .map_err(|error| error.to_string())
        });
        let begun = std::time::Instant::now();
        while !flags.path().join("started").exists() {
            assert!(
                begun.elapsed() < WAIT,
                "the first ship never reached its gate"
            );
            thread::sleep(POLL);
        }
        // The first ship is inside its gates: the lock is free, so the second takes it with no
        // wait at all, runs its gates and pushes.
        let options = with_lock(&second, offline());
        let pushed = ship_with(&second, &options, &|_, _, _, _, _| Err("no review".into()))
            .expect("the second ship, while the first runs its gates");
        assert_eq!(remote_tip(&root, "feature2"), pushed.head);
        drop(options);
        std::fs::write(flags.path().join("go"), "").unwrap();
        let head = first.join().unwrap().expect("the first ship");
        assert_eq!(remote_tip(&root, "feature"), head);
    });
}
