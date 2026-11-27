//! The child `make` of ship: scripted to write a result, to wait for a slot, or to run on.
use super::*;
use std::os::unix::fs::PermissionsExt;

/// A scripted `make`: it lists the `game-score-pr` target and otherwise runs `body`, with
/// `$MARK` naming a scratch directory and the flags in `$GAME_SCORE_PR_ARGS`.
fn scripted(body: &str, with_target: bool) -> (tempfile::TempDir, PathBuf) {
    let (temp, root) = fixture("true");
    let make = temp.path().join("make");
    let listing = if with_target {
        "game-score-pr:"
    } else {
        "other:"
    };
    fs::write(
        &make,
        format!(
            "#!/bin/sh\nif [ \"$1\" = -rRqp ]; then echo '{listing}'; exit 0; fi\nMARK={mark}\n{body}\n",
            mark = temp.path().display()
        ),
    )
    .expect("script");
    fs::set_permissions(&make, fs::Permissions::from_mode(0o755)).expect("mode");
    MAKE.with(|slot| *slot.borrow_mut() = Some(make.to_string_lossy().into_owned()));
    (temp, root)
}

/// What the resource layer prints when it gives a job its slot.
const ADMIT: &str = "echo 'long job admitted: test' >&2";

fn writes(outcome: &Outcome) -> String {
    format!(
        "{ADMIT}\nmkdir -p .cache/game-score\nprintf '%s' '{}' > .cache/game-score/pr.json",
        result_json(outcome)
    )
}

/// Waits until the child has ended or holds its slot: the moment the review would end in ship.
fn settled(started: &Started) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while started.running() && !started.admitted.load(Ordering::SeqCst) && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(20));
    }
}

fn count(temp: &tempfile::TempDir) -> u32 {
    fs::read_to_string(temp.path().join("count"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

#[test]
fn a_child_make_writes_the_result_and_finish_reads_it() {
    let (temp, root) = scripted(
        &format!(
            "echo \"$@\" > \"$MARK/invoked\"\necho \"$GAME_SCORE_PR_ARGS\" > \"$MARK/flags\"\n{}",
            writes(&outcome(true))
        ),
        true,
    );
    let body = temp.path().join("body.md");
    fs::write(&body, "game-score: late arrived -3% because r\n").expect("body");
    let started = start_with(&root, HEAD, BASE, Some(&body)).expect("started");
    settled(&started);
    let found = started.finish().expect("a result");
    assert!(found.ran);
    assert_eq!(found.head, HEAD);
    assert!(
        found
            .section
            .contains("- this check added 0 s to this ship beyond the review"),
        "it ended before the review did: {}",
        found.section
    );
    assert!(!found.section.contains(ADDED), "the marker is replaced");
    assert_eq!(
        fs::read_to_string(temp.path().join("invoked"))
            .expect("invoked")
            .trim(),
        "--no-print-directory game-score-pr"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("flags"))
            .expect("flags")
            .trim(),
        arguments(BASE, true)
    );
    assert_eq!(
        fs::read_to_string(root.join(".cache/game-score/description.md")).expect("copy"),
        "game-score: late arrived -3% because r\n"
    );
}

#[test]
fn an_old_result_is_never_taken_for_the_new_run() {
    let (_temp, root) = scripted("exit 0", true);
    fs::create_dir_all(root.join(".cache/game-score")).expect("folder");
    fs::write(
        root.join(".cache/game-score/pr.json"),
        result_json(&outcome(true)),
    )
    .expect("old");
    let started = start_with(&root, HEAD, BASE, None).expect("started");
    settled(&started);
    assert!(started.finish().is_none(), "the old file was removed first");
}

const BUSY: &str = "echo 'long job busy: slot 0: x, retry later' >&2\nexit 2";
const COUNT: &str =
    "n=$(cat \"$MARK/count\" 2>/dev/null || echo 0)\necho $((n + 1)) > \"$MARK/count\"";

#[test]
fn a_busy_machine_is_asked_again_while_the_review_runs_until_a_slot_is_free() {
    let (temp, root) = scripted(
        &format!(
            "{COUNT}\nif [ \"$n\" -lt 2 ]; then {BUSY}; fi\n{}",
            writes(&outcome(true))
        ),
        true,
    );
    let started = start_with(&root, HEAD, BASE, None).expect("started");
    // The review is still running: the child asks again by itself.
    let deadline = Instant::now() + Duration::from_secs(10);
    while count(&temp) < 3 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    settled(&started);
    assert!(started.finish().is_some_and(|found| found.ran));
    assert_eq!(count(&temp), 3);
}

#[test]
fn a_check_still_waiting_for_a_slot_when_the_review_ends_is_stopped_at_once() {
    let (temp, root) = scripted(&format!("{COUNT}\n{BUSY}"), true);
    let started = start_with(&root, HEAD, BASE, None).expect("started");
    let deadline = Instant::now() + Duration::from_secs(10);
    while count(&temp) < 1 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let waited = Instant::now();
    assert!(started.finish().is_none());
    assert!(
        waited.elapsed() < Duration::from_millis(900),
        "{:?}",
        waited.elapsed()
    );
    let at_the_end = count(&temp);
    thread::sleep(Duration::from_millis(500));
    assert_eq!(count(&temp), at_the_end, "it does not ask again");
}

#[test]
fn a_check_that_runs_is_given_a_while_after_the_review_then_stopped() {
    let (temp, root) = scripted(
        &format!("{ADMIT}\nsleep 60 &\necho $! > \"$MARK/sleeper\"\necho $$ > \"$MARK/pid\"\nwait"),
        true,
    );
    let started = start_with(&root, HEAD, BASE, None).expect("started");
    settled(&started);
    assert!(started.admitted.load(Ordering::SeqCst));
    let waited = Instant::now();
    assert!(started.finish().is_none());
    let took = waited.elapsed();
    assert!(
        took >= GRACE && took < Duration::from_secs(20),
        "the grace of the test is {GRACE:?}: {took:?}"
    );
    for file in ["pid", "sleeper"] {
        let pid = fs::read_to_string(temp.path().join(file)).expect(file);
        let deadline = Instant::now() + Duration::from_secs(10);
        while running(pid.trim()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        assert!(!running(pid.trim()), "{file} {pid} still runs");
    }
}

#[test]
fn a_check_that_ends_within_the_wait_is_read_and_says_what_it_added() {
    let (_temp, root) = scripted(
        &format!("{ADMIT}\nsleep 0.5\n{}", writes(&outcome(true))),
        true,
    );
    let started = start_with(&root, HEAD, BASE, None).expect("started");
    settled(&started);
    let found = started.finish().expect("it ended inside the wait");
    let line = found
        .section
        .lines()
        .find(|line| line.starts_with("- this check added "))
        .expect("the line");
    assert!(
        line.ends_with(" s to this ship beyond the review (0 when it ended before the review did)"),
        "{line}"
    );
}

#[test]
fn a_failure_that_is_not_a_busy_slot_is_not_retried_and_leaves_no_result() {
    let (temp, root) = scripted(
        "n=$(cat \"$MARK/count\" 2>/dev/null || echo 0)\necho $((n + 1)) > \"$MARK/count\"\necho 'error[E0432]' >&2\nexit 2",
        true,
    );
    let started = start_with(&root, HEAD, BASE, None).expect("started");
    settled(&started);
    assert!(started.finish().is_none());
    assert_eq!(count(&temp), 1);
    let log = fs::read_to_string(root.join(".cache/game-score/pr.log")).expect("log");
    assert!(log.contains("error[E0432]"));
}

#[test]
fn a_result_for_another_commit_is_no_result() {
    let mut other = outcome(true);
    other.head = BASE.to_owned();
    let (_temp, root) = scripted(&writes(&other), true);
    let started = start_with(&root, HEAD, BASE, None).expect("started");
    settled(&started);
    assert!(started.finish().is_none());
}

#[test]
fn a_revision_without_the_target_starts_nothing() {
    let (_temp, root) = scripted("exit 1", false);
    assert!(start_with(&root, HEAD, BASE, None).is_none());
}

#[test]
fn only_a_ship_that_makes_a_pull_request_starts_it() {
    let (_temp, root) = scripted("exit 0", true);
    let evidence = evidence();
    assert!(
        start(&root, &evidence, &offline()).is_none(),
        "no pull request"
    );
    let ahead = Options {
        no_pr: false,
        review_ahead: true,
        ..offline()
    };
    assert!(
        start(&root, &evidence, &ahead).is_none(),
        "review ahead publishes nothing"
    );
}

#[test]
fn a_skipped_run_is_a_result_without_a_table() {
    let mut skipped = outcome(false);
    skipped.summary =
        "skipped because no changed crate is in the build closure of aoe-server".to_owned();
    let (_temp, root) = scripted(&writes(&skipped), true);
    let started = start_with(&root, HEAD, BASE, None).expect("started");
    settled(&started);
    let found = started.finish().expect("result");
    assert!(!found.ran);
}

fn running(pid: &str) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        !stat
            .rsplit_once(')')
            .is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'))
    })
}

#[test]
fn dropping_it_ends_the_child_and_whatever_it_started() {
    let (temp, root) = scripted(
        "sleep 60 &\necho $! > \"$MARK/sleeper\"\necho $$ > \"$MARK/pid\"\nwait",
        true,
    );
    let started = start_with(&root, HEAD, BASE, None).expect("started");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(temp.path().join("sleeper").exists() && temp.path().join("pid").exists())
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(50));
    }
    drop(started);
    for file in ["pid", "sleeper"] {
        let pid = fs::read_to_string(temp.path().join(file)).expect(file);
        let deadline = Instant::now() + Duration::from_secs(10);
        while running(pid.trim()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        assert!(!running(pid.trim()), "{file} {pid} still runs");
    }
}
