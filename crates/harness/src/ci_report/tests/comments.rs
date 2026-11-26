use super::fixture;
use crate::ci_report::calendar;
use crate::ci_report::comments::{parse_comment, parse_duration};

#[test]
fn durations_parse_with_and_without_hours_and_minutes() {
    assert_eq!(parse_duration("1h02m"), Some(3_720));
    assert_eq!(parse_duration("16m00s"), Some(960));
    assert_eq!(parse_duration("47s"), Some(47));
    assert_eq!(parse_duration("1h"), Some(3_600));
    assert_eq!(parse_duration(" 2m05s "), Some(125));
    assert_eq!(parse_duration("—"), None);
    assert_eq!(parse_duration(""), None);
    assert_eq!(parse_duration("soon"), None);
}

#[test]
fn a_comment_that_does_not_start_with_the_marker_is_not_ours() {
    assert!(parse_comment(1, "2026-11-01T00:00:00Z", "hello\n").is_none());
    let quoted = format!("quote: {}", fixture("new_latest"));
    assert!(parse_comment(1, "2026-11-01T00:00:00Z", &quoted).is_none());
}

#[test]
fn the_new_format_gives_both_revisions_the_dev_column_and_the_jobs() {
    let seen = parse_comment(1072, "2026-11-25T12:32:38Z", &fixture("new_latest")).expect("ours");
    assert_eq!(seen.run, Some(45_604_793_989));
    let dev = seen.dev.expect("a dev side");
    assert_eq!(dev.rev, "0922a8fdbc8bbe1e864a22f28fc1013abd0ef747");
    assert_eq!(dev.rows["simulation::late_game"], 9_696_224_729);
    assert_eq!(dev.rows["simulation::late_game_x4_critical"], 2_843_761_484);
    assert_eq!(dev.rows["wasm_gzip"], 455_290);
    assert_eq!(dev.rows["wasm_raw"], 1_045_308);
    assert_eq!(dev.rows.len(), 14, "12 instruction rows and 2 bundle rows");
    assert!(
        dev.rows.keys().all(|row| !row.starts_with("Tick")),
        "timing rows stay out"
    );
    assert_eq!(seen.jobs.len(), 10);
    let slowest = seen.jobs.iter().find(|job| job.name == "e2e (1)").unwrap();
    assert_eq!((slowest.seconds, slowest.failed), (1_887, false));
}

#[test]
fn the_old_format_parses_the_same_table() {
    let seen = parse_comment(405, "2026-10-14T21:26:08Z", &fixture("old_with_dev")).expect("ours");
    let dev = seen.dev.expect("a dev side");
    assert_eq!(dev.rev, "fd44b489e377f636f72feea968ce9944c9d2d9e3");
    assert_eq!(dev.rows["simulation::advance"], 675_830);
    assert_eq!(dev.rows["wasm_gzip"], 265_980);
    assert_eq!(dev.rows.len(), 6);
    assert_eq!(seen.run, Some(38_639_082_143));
    assert_eq!(seen.jobs.len(), 3);
}

#[test]
fn a_comment_without_a_dev_run_keeps_its_jobs_and_no_dev_side() {
    let seen = parse_comment(410, "2026-10-20T18:20:08Z", &fixture("old_no_dev")).expect("ours");
    assert!(seen.dev.is_none(), "its dev column is all dashes");
    assert_eq!(seen.run, Some(39_472_486_345));
    assert_eq!(seen.jobs.len(), 5);
}

#[test]
fn a_comment_of_a_run_that_measured_nothing_still_lists_job_durations() {
    let seen = parse_comment(595, "2026-10-27T10:36:57Z", &fixture("no_artifact")).expect("ours");
    assert!(seen.dev.is_none());
    assert_eq!(seen.run, Some(40_488_728_501));
    assert_eq!(seen.jobs.len(), 4);
}

#[test]
fn a_failed_job_keeps_its_name_and_is_flagged() {
    let old = parse_comment(413, "2026-10-15T00:46:08Z", &fixture("old_failed_job")).unwrap();
    let static_job = old.jobs.iter().find(|job| job.name == "static").unwrap();
    assert_eq!((static_job.seconds, static_job.failed), (960, true));
    let new = parse_comment(661, "2026-11-02T20:50:14Z", &fixture("new_failed_checks")).unwrap();
    let checks = new.jobs.iter().find(|job| job.name == "static (checks)");
    let checks = checks.expect("the shard name keeps its parentheses");
    assert_eq!((checks.seconds, checks.failed), (1_160, true));
    assert_eq!(new.jobs.iter().filter(|job| job.failed).count(), 1);
}

#[test]
fn utc_calendar_helpers() {
    assert_eq!(calendar::iso(1_795_609_958), "2026-11-25T12:32:38Z");
    assert_eq!(calendar::day("2026-11-25T12:32:38Z"), "2026-11-25");
    // Thursday, Sunday, Monday, and a year boundary (Friday 2027-01-01).
    assert_eq!(
        calendar::week_start("2026-11-26T08:00:00Z").unwrap(),
        "2026-11-23"
    );
    assert_eq!(
        calendar::week_start("2026-11-29T23:59:59Z").unwrap(),
        "2026-11-23"
    );
    assert_eq!(
        calendar::week_start("2026-11-23T00:00:00Z").unwrap(),
        "2026-11-23"
    );
    assert_eq!(
        calendar::week_start("2027-01-01T10:00:00Z").unwrap(),
        "2026-12-28"
    );
    assert!(calendar::week_start("not a date").is_none());
}
