use crate::ci_report::comments::{JobTime, Observation, parse_comment};
use crate::ci_report::timing::{daily, median, runs};

fn job(name: &str, seconds: u64, failed: bool) -> JobTime {
    JobTime {
        name: name.to_owned(),
        seconds,
        failed,
    }
}

fn run(id: Option<u64>, at: &str, jobs: Vec<JobTime>) -> Observation {
    Observation {
        pr: 7,
        at: at.to_owned(),
        run: id,
        dev: None,
        jobs,
    }
}

#[test]
fn a_median_of_an_even_count_is_the_middle_pair_average() {
    assert_eq!(median(&mut [5.0, 1.0, 3.0]), 3.0);
    assert_eq!(median(&mut [4.0, 1.0, 3.0, 10.0]), 3.5);
    assert_eq!(median(&mut []), 0.0);
}

#[test]
fn a_run_keeps_its_longest_job_and_its_machine_time() {
    let found = runs(&[run(
        Some(1),
        "2026-11-20T10:00:00Z",
        vec![
            job("select", 120, false),
            job("browser", 600, false),
            job("static (unit)", 900, false),
        ],
    )]);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].longest, found[0].total), (900, 1_620));
    assert_eq!(found[0].jobs["browser"], 600);
}

#[test]
fn e2e_shards_collapse_into_the_slowest_one() {
    let found = runs(&[run(
        Some(1),
        "2026-11-20T10:00:00Z",
        vec![
            job("e2e (1)", 1_500, false),
            job("e2e (2)", 1_900, false),
            job("e2e (3)", 1_700, false),
        ],
    )]);
    assert_eq!(found[0].jobs["e2e"], 1_900);
    assert_eq!(found[0].total, 5_100, "every shard used a machine");
}

#[test]
fn a_failed_job_leaves_its_column_but_counts_in_longest_and_total() {
    let found = runs(&[run(
        Some(1),
        "2026-11-20T10:00:00Z",
        vec![
            job("static (checks)", 1_160, true),
            job("browser", 600, false),
        ],
    )]);
    assert!(!found[0].jobs.contains_key("static (checks)"));
    assert_eq!((found[0].longest, found[0].total), (1_160, 1_760));
    assert_eq!(found[0].failed, ["static (checks)"]);
}

#[test]
fn one_run_is_counted_once_even_when_two_versions_of_a_comment_list_it() {
    let early = run(
        Some(9),
        "2026-11-20T10:00:00Z",
        vec![job("browser", 100, false)],
    );
    let late = run(
        Some(9),
        "2026-11-20T11:00:00Z",
        vec![job("browser", 200, false)],
    );
    let other = run(
        Some(10),
        "2026-11-20T12:00:00Z",
        vec![job("browser", 300, false)],
    );
    let found = runs(&[late, other, early]);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].jobs["browser"], 200, "the later version wins");
    assert_eq!(found[1].jobs["browser"], 300);
    assert!(runs(&[run(Some(1), "2026-11-20T10:00:00Z", vec![])]).is_empty());
}

#[test]
fn daily_medians_are_per_job_per_utc_day() {
    let at = |day: &str, hour: u32| format!("2026-11-{day}T{hour:02}:00:00Z");
    let found = runs(&[
        run(
            Some(1),
            &at("20", 9),
            vec![job("browser", 600, false), job("select", 60, false)],
        ),
        run(Some(2), &at("20", 23), vec![job("browser", 1_200, false)]),
        run(Some(3), &at("20", 12), vec![job("browser", 900, false)]),
        run(
            Some(4),
            &at("21", 1),
            vec![job("browser", 300, true), job("select", 120, false)],
        ),
    ]);
    let days = daily(&found);
    assert_eq!(days.len(), 2);
    assert_eq!((days[0].day.as_str(), days[0].runs), ("2026-11-20", 3));
    assert_eq!(
        days[0].jobs["browser"], 15.0,
        "median of 10, 15 and 20 minutes"
    );
    assert_eq!(
        days[0].longest, 15.0,
        "the longest job of each run: 10, 20 and 15 minutes"
    );
    assert_eq!(
        days[0].total, 15.0,
        "machine time per run: 11, 20 and 15 minutes"
    );
    assert!(
        !days[1].jobs.contains_key("browser"),
        "only a failed run that day"
    );
    assert_eq!(days[1].jobs["select"], 2.0);
    assert_eq!((days[1].longest, days[1].total), (5.0, 7.0));
}

#[test]
fn real_comments_become_runs() {
    let body = super::fixture("new_latest");
    let seen = parse_comment(1072, "2026-11-25T12:32:38Z", &body).unwrap();
    let found = runs(&[seen]);
    assert_eq!(found[0].jobs["e2e"], 1_887);
    assert_eq!(found[0].jobs["target-performance"], 3_164);
    assert_eq!(found[0].longest, 3_164);
    assert_eq!(found[0].total, 12_758);
}
