use crate::ci_report::BundleLimit;
use crate::ci_report::comments::{DevSide, Observation};
use crate::ci_report::history::{Accepted, Event, EventKind, Version};
use crate::ci_report::series::{
    Commit, DevPoint, State, dev_points, moves, parse_commits, segments, standing,
};
use std::collections::BTreeMap;

fn sha(index: usize) -> String {
    format!("{index:040x}")
}

fn commits(count: usize) -> Vec<Commit> {
    (0..count)
        .map(|i| Commit {
            sha: sha(i),
            at: format!("2026-11-{:02}T12:00:00Z", i + 1),
            subject: format!("change {i} (#{})", 500 + i),
        })
        .collect()
}

fn observation(index: usize, rows: &[(&str, u64)]) -> Observation {
    Observation {
        pr: 900,
        at: format!("2026-11-{:02}T13:00:00Z", index + 2),
        run: None,
        dev: Some(DevSide {
            rev: sha(index),
            rows: rows.iter().map(|(r, v)| ((*r).to_owned(), *v)).collect(),
        }),
        jobs: Vec::new(),
    }
}

fn points(values: &[(usize, u64)]) -> Vec<DevPoint> {
    let observed: Vec<_> = values
        .iter()
        .map(|&(index, value)| observation(index, &[("a::x", value)]))
        .collect();
    dev_points(&commits(12), &observed)
}

fn redefined(position: usize) -> Event {
    Event {
        id: "A".into(),
        kind: EventKind::Redefined,
        rows: vec!["a::x".into()],
        position,
        at: String::new(),
        pr: None,
        title: String::new(),
        what: String::new(),
    }
}

#[test]
fn the_git_log_becomes_commits_oldest_first_with_utc_dates() {
    let log = format!(
        "{}\t1795609958\tfirst (#7)\n{}\t1795600000\tsecond\n",
        sha(2),
        sha(1)
    );
    let found = parse_commits(&log).unwrap();
    assert_eq!(
        found[0].sha,
        sha(1),
        "the log lists the newest commit first"
    );
    assert_eq!(found[1].at, "2026-11-25T12:32:38Z");
    assert_eq!(found[1].subject, "first (#7)");
    assert!(parse_commits("garbage").is_err());
}

#[test]
fn dev_points_follow_the_commit_order_and_count_the_commits_between() {
    let found = points(&[(5, 100), (2, 90), (9, 110)]);
    let order: Vec<_> = found.iter().map(|p| p.position).collect();
    assert_eq!(order, [2, 5, 9]);
    assert_eq!(found[0].since_previous, None);
    assert_eq!(found[1].since_previous, Some(3));
    assert_eq!(found[1].pr, Some(505));
    assert_eq!(found[1].title, "change 5");
    assert_eq!(found[1].rev, &sha(5)[..9]);
}

#[test]
fn a_revision_outside_the_log_is_skipped_and_the_later_comment_wins() {
    let mut stray = observation(3, &[("a::x", 1)]);
    stray.dev.as_mut().unwrap().rev = "f".repeat(40);
    let early = observation(3, &[("a::x", 100), ("a::old", 7)]);
    let mut late = observation(3, &[("a::x", 101)]);
    late.at = "2026-12-01T00:00:00Z".into();
    let found = dev_points(&commits(5), &[late, stray, early]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rows["a::x"], 101);
    assert_eq!(found[0].rows["a::old"], 7, "rows from every comment merge");
}

#[test]
fn a_redefinition_splits_a_rows_line_at_its_commit() {
    let found = points(&[(1, 10), (3, 11), (5, 90), (8, 95)]);
    assert_eq!(segments(&found, "a::x", &[]), vec![vec![0, 1, 2, 3]]);
    let split = segments(&found, "a::x", &[redefined(5)]);
    assert_eq!(split, vec![vec![0, 1], vec![2, 3]]);
    let accepted = Event {
        kind: EventKind::Accepted,
        ..redefined(5)
    };
    assert_eq!(segments(&found, "a::x", &[accepted]).len(), 1);
}

#[test]
fn a_move_of_exactly_two_percent_counts_and_less_does_not() {
    let found = points(&[(1, 1_000), (2, 1_020), (3, 1_039), (4, 1_000)]);
    let all = moves(&found, &[]);
    let changes: Vec<_> = all.iter().map(|m| (m.after, m.before)).collect();
    assert_eq!(
        changes,
        vec![(1_000, 1_039), (1_020, 1_000)],
        "newest first"
    );
    assert!((all[1].change - 2.0).abs() < 1e-9);
    assert_eq!(all[1].pr, Some(502));
    assert_eq!(all[1].commits_since, 1);
}

#[test]
fn a_move_across_a_redefinition_or_an_accepted_increase_is_flagged() {
    let found = points(&[(1, 100), (4, 900), (7, 1_000)]);
    let accepted = Event {
        kind: EventKind::Accepted,
        position: 7,
        ..redefined(7)
    };
    let all = moves(&found, &[redefined(3), accepted]);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].event, Some(EventKind::Accepted));
    assert_eq!(all[1].event, Some(EventKind::Redefined));
    assert_eq!(all[1].commits_since, 3);
}

#[test]
fn a_row_missing_from_a_commit_compares_with_its_last_measurement() {
    let mut observed = vec![observation(1, &[("a::x", 100), ("a::y", 100)])];
    observed.push(observation(2, &[("a::y", 100)]));
    observed.push(observation(3, &[("a::x", 130), ("a::y", 100)]));
    let found = moves(&dev_points(&commits(5), &observed), &[]);
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].row.as_str(), found[0].before, found[0].after),
        ("a::x", 100, 130)
    );
    assert_eq!(found[0].commits_since, 2);
}

fn baseline(row: &str, now: u64, reference: u64) -> Version {
    Version {
        instructions: BTreeMap::from([(row.to_owned(), now)]),
        reference: BTreeMap::from([(row.to_owned(), reference)]),
        ..Version::default()
    }
}

fn state_at(value: u64) -> State {
    let observed = [observation(4, &[("a::x", value)])];
    let found = dev_points(&commits(6), &observed);
    standing(&found, &[baseline("a::x", 1_000, 1_000)], &[])[0].state
}

#[test]
fn a_gated_row_passes_warns_fails_or_asks_for_a_new_baseline() {
    assert_eq!(state_at(1_000), State::Passes);
    assert_eq!(
        state_at(1_010),
        State::Passes,
        "one percent exactly is not over"
    );
    assert_eq!(state_at(1_011), State::Warns);
    assert_eq!(state_at(1_031), State::Fails);
    assert_eq!(state_at(960), State::NewBaselineDue);
}

#[test]
fn a_row_over_its_fixed_reference_fails_whatever_its_baseline_says() {
    let observed = [observation(4, &[("a::x", 1_000)])];
    let found = dev_points(&commits(6), &observed);
    let rows = standing(&found, &[baseline("a::x", 1_000, 800)], &[]);
    assert_eq!(rows[0].state, State::Fails);
}

#[test]
fn a_kernel_with_a_main_thread_row_allows_ten_percent_on_its_total() {
    let observed = [observation(4, &[("a::k", 1_080), ("a::k_main", 1_080)])];
    let found = dev_points(&commits(6), &observed);
    let mut base = baseline("a::k", 1_000, 1_000);
    base.instructions.insert("a::k_main".into(), 1_000);
    base.reference.insert("a::k_main".into(), 1_000);
    let rows = standing(&found, &[base], &[]);
    let state = |row: &str| rows.iter().find(|r| r.row == row).unwrap().state;
    assert_eq!(rows[0].limit_percent, 10);
    assert_eq!(state("a::k"), State::Warns);
    assert_eq!(state("a::k_main"), State::Fails);
}

#[test]
fn standing_reports_the_accepted_level_the_day_change_and_the_bundle() {
    let observed = [
        observation(1, &[("a::x", 1_000), ("wasm_gzip", 400)]),
        observation(4, &[("a::x", 1_100), ("wasm_gzip", 500)]),
    ];
    let found = dev_points(&commits(6), &observed);
    let mut base = baseline("a::x", 1_000, 2_000);
    base.accepted.insert(
        "a::x".into(),
        Accepted {
            dev: 1_090,
            reason: String::new(),
        },
    );
    let limit = BundleLimit {
        position: 0,
        baseline: 400,
        limit: 499,
    };
    let rows = standing(&found, &[base], &[limit]);
    let micro = &rows[0];
    assert_eq!(
        (micro.row.as_str(), micro.now, micro.baseline),
        ("a::x", 1_100, 1_000)
    );
    assert_eq!(micro.accepted_from, Some(1_090));
    assert!((micro.day_change.unwrap() - 10.0).abs() < 1e-9);
    let bundle = rows.last().unwrap();
    assert_eq!(
        (bundle.row.as_str(), bundle.limit_percent),
        ("wasm_gzip", 25)
    );
    assert_eq!(bundle.state, State::Passes, "500 is exactly 25% over 400");
}

#[test]
fn a_baseline_committed_after_the_measured_commit_is_not_applied_to_it() {
    // The comment names commit 4; a redefinition landed at commit 6 since.
    let observed = [observation(4, &[("a::x", 1_000)])];
    let found = dev_points(&commits(8), &observed);
    let mut newer = baseline("a::x", 5_000, 5_000);
    newer.position = 6;
    let older = baseline("a::x", 1_000, 1_000);
    let rows = standing(&found, &[older.clone(), newer.clone()], &[]);
    assert_eq!((rows[0].baseline, rows[0].state), (1_000, State::Passes));
    let later = [observation(7, &[("a::x", 5_000)])];
    let found = dev_points(&commits(8), &later);
    let rows = standing(&found, &[older, newer], &[]);
    assert_eq!((rows[0].baseline, rows[0].state), (5_000, State::Passes));
}
