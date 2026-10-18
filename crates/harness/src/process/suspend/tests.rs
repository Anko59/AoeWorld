use super::{Reading, THRESHOLD, Watch, containers, moved};
use crate::process::{Cancellation, CaptureExit, capture::capture_command_at};
use std::{
    cell::Cell,
    io,
    process::Command,
    time::{Duration, Instant},
};

const HOUR: Duration = Duration::from_secs(3600);

/// A clock whose gap is `gap` seconds; the monotonic clock never moves.
fn gap_clock(gap: &Cell<u64>) -> impl Fn() -> io::Result<Reading> + '_ {
    move || {
        Ok(Reading {
            boot: Duration::from_secs(gap.get()),
            monotonic: Duration::ZERO,
        })
    }
}

#[test]
fn the_gap_is_boot_time_that_monotonic_time_does_not_count() {
    let reading = Reading {
        boot: Duration::from_secs(100),
        monotonic: Duration::from_secs(40),
    };
    assert_eq!(reading.gap(), Duration::from_secs(60));
}

#[test]
fn a_gap_that_moves_by_more_than_the_threshold_is_a_suspend() {
    let recorded = HOUR;
    assert!(
        !moved(recorded, recorded + THRESHOLD),
        "a boundary is not a suspend"
    );
    assert!(moved(
        recorded,
        recorded + THRESHOLD + Duration::from_secs(1)
    ));
    assert!(!moved(
        recorded,
        recorded.saturating_sub(Duration::from_secs(59))
    ));
    // A clock that moved back is as unexplained as one that moved forward.
    assert!(moved(
        recorded,
        recorded.saturating_sub(Duration::from_secs(61))
    ));
}

#[test]
fn a_run_is_interrupted_only_once_the_gap_has_moved() {
    let gap = Cell::new(3600);
    let clock = gap_clock(&gap);
    let watch = Watch::start(&clock);
    assert_eq!(watch.gap_seconds(), Some(3600));
    gap.set(3600 + 30);
    assert!(!watch.interrupted(&clock), "a short pause is not a suspend");
    gap.set(3600 + 61);
    assert!(watch.interrupted(&clock));
}

#[test]
fn a_run_without_a_readable_clock_is_never_judged_interrupted() {
    let broken = || -> io::Result<Reading> { Err(io::Error::other("no clock")) };
    let watch = Watch::start(&broken);
    assert_eq!(watch.gap_seconds(), None);
    assert!(!watch.interrupted(&broken));
}

#[test]
fn a_child_inherits_the_gap_it_started_at() {
    let gap = Cell::new(42);
    let clock = gap_clock(&gap);
    let mut command = Command::new("sh");
    command.args(["-c", "printf %s \"$AOE_HARNESS_BOOT_GAP\""]);
    let captured = capture_command_at(
        command,
        Duration::from_secs(20),
        &Cancellation::default(),
        &clock,
    );
    assert!(
        matches!(captured.exit, CaptureExit::Success),
        "{:?}",
        captured.exit
    );
    assert_eq!(captured.stdout, b"42");
}

#[test]
fn a_suspend_while_a_child_runs_kills_it_and_reports_the_suspend() {
    let begun = Instant::now();
    // The gap stays put for a moment, so the child is really running, then a suspend lands.
    let clock = move || -> io::Result<Reading> {
        let gap = if begun.elapsed() > Duration::from_millis(300) {
            120
        } else {
            0
        };
        Ok(Reading {
            boot: Duration::from_secs(gap),
            monotonic: Duration::ZERO,
        })
    };
    let mut command = Command::new("sleep");
    command.arg("30");
    let captured = capture_command_at(
        command,
        Duration::from_secs(60),
        &Cancellation::default(),
        &clock,
    );
    assert!(
        matches!(&captured.exit, CaptureExit::Suspended { at } if at.ends_with('Z')),
        "{:?}",
        captured.exit
    );
    assert!(
        captured.duration < Duration::from_secs(20),
        "the child is killed at once, not left to its 30 s: {:?}",
        captured.duration
    );
}

#[test]
fn a_failed_removal_does_not_stop_the_others_and_is_reported() {
    let stale = vec![
        ("a".to_owned(), 10),
        ("b".to_owned(), 10),
        ("c".to_owned(), 10),
    ];
    let mut tried = Vec::new();
    let result = containers::stop_all(&stale, |id| {
        tried.push(id.to_owned());
        if id == "b" {
            Err("no such container".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(tried, ["a", "b", "c"]);
    let error = result.expect_err("b could not be removed");
    assert!(error.contains("b: no such container"), "{error}");
}

#[test]
fn only_containers_whose_start_gap_moved_are_interrupted() {
    let listing = "abc\t10\ndef\t100\nghi\t\nxyz\tnot-a-gap\n";
    // Now the gap is 100 s: `abc` started 90 s before it moved; `def` is unchanged.
    let stale = containers::interrupted(listing, Duration::from_secs(100));
    assert_eq!(stale, vec![("abc".to_owned(), 10)]);
}
