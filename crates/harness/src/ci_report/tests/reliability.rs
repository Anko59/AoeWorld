use crate::ci_report::actions::{ActionJob, ActionRun, parse_jobs, parse_runs, reliability};

fn jobs(list: &[(&str, &str)]) -> Vec<ActionJob> {
    list.iter()
        .map(|(name, conclusion)| ActionJob {
            name: (*name).to_owned(),
            conclusion: Some((*conclusion).to_owned()),
            step: (*conclusion == "failure").then(|| "Run tests".to_owned()),
        })
        .collect()
}

fn run(id: u64, at: &str, conclusion: &str, list: &[(&str, &str)]) -> ActionRun {
    ActionRun {
        id,
        attempt: 1,
        conclusion: Some(conclusion.to_owned()),
        created_at: at.to_owned(),
        jobs: jobs(list),
        first_attempt: None,
    }
}

#[test]
fn the_share_of_runs_that_failed_is_counted_per_job_and_per_week() {
    let found = reliability(&[
        run(
            1,
            "2026-11-17T10:00:00Z",
            "success",
            &[("browser", "success"), ("static", "success")],
        ),
        run(
            2,
            "2026-11-18T10:00:00Z",
            "failure",
            &[("browser", "success"), ("static", "failure")],
        ),
        run(
            3,
            "2026-11-19T10:00:00Z",
            "success",
            &[("browser", "success"), ("static", "success")],
        ),
        run(
            4,
            "2026-11-24T10:00:00Z",
            "failure",
            &[("browser", "failure"), ("static", "success")],
        ),
    ]);
    assert_eq!(found.weeks, ["2026-11-16", "2026-11-23"]);
    assert_eq!((found.runs, found.runs_failed), (4, 2));
    let stat = found.jobs.iter().find(|job| job.name == "static").unwrap();
    assert_eq!((stat.runs, stat.failed), (4, 1));
    assert_eq!(stat.weeks["2026-11-16"], (3, 1));
    assert_eq!(stat.weeks["2026-11-23"], (1, 0));
    let browser = found.jobs.iter().find(|job| job.name == "browser").unwrap();
    assert_eq!(browser.weeks["2026-11-23"], (1, 1));
}

#[test]
fn end_to_end_shards_count_as_one_job_that_failed_if_any_shard_did() {
    let found = reliability(&[run(
        1,
        "2026-11-17T10:00:00Z",
        "failure",
        &[
            ("e2e (1)", "success"),
            ("e2e (2)", "failure"),
            ("e2e (3)", "success"),
        ],
    )]);
    assert_eq!(found.jobs.len(), 1);
    assert_eq!(
        (
            found.jobs[0].name.as_str(),
            found.jobs[0].runs,
            found.jobs[0].failed
        ),
        ("e2e", 1, 1)
    );
}

#[test]
fn cancelled_and_skipped_jobs_say_nothing_about_reliability() {
    let found = reliability(&[run(
        1,
        "2026-11-17T10:00:00Z",
        "cancelled",
        &[
            ("browser", "cancelled"),
            ("static", "skipped"),
            ("select", "success"),
        ],
    )]);
    let names: Vec<_> = found.jobs.iter().map(|job| job.name.as_str()).collect();
    assert_eq!(names, ["select"]);
    assert_eq!(found.runs_failed, 0);
}

#[test]
fn a_run_that_failed_then_passed_on_a_rerun_is_credited_to_the_job_that_failed() {
    let mut rerun = run(
        1,
        "2026-11-17T10:00:00Z",
        "success",
        &[("browser", "success"), ("static", "success")],
    );
    rerun.attempt = 2;
    rerun.first_attempt = Some(jobs(&[("browser", "failure"), ("static", "success")]));
    let mut still_red = run(
        2,
        "2026-11-17T11:00:00Z",
        "failure",
        &[("browser", "failure"), ("static", "success")],
    );
    still_red.attempt = 2;
    still_red.first_attempt = Some(jobs(&[("browser", "failure"), ("static", "success")]));
    let found = reliability(&[rerun, still_red]);
    assert_eq!(
        (found.runs, found.runs_failed, found.runs_recovered),
        (2, 1, 1)
    );
    let browser = found.jobs.iter().find(|job| job.name == "browser").unwrap();
    assert_eq!(browser.recovered, 1);
    assert_eq!(
        (browser.runs, browser.failed),
        (2, 1),
        "the final attempt is what stands"
    );
    assert_eq!(browser.step.as_deref(), Some("Run tests"));
    let stat = found.jobs.iter().find(|job| job.name == "static").unwrap();
    assert_eq!(stat.recovered, 0);
}

#[test]
fn the_api_replies_parse_into_runs_and_jobs() {
    let runs = parse_runs(
        r#"{"total_count": 2, "workflow_runs": [
            {"id": 11, "run_attempt": 2, "conclusion": "success", "status": "completed", "created_at": "2026-11-20T10:00:00Z"},
            {"id": 12, "run_attempt": 1, "conclusion": null, "status": "in_progress", "created_at": "2026-11-20T11:00:00Z"}
        ]}"#,
    )
    .unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!((runs[0].id, runs[0].attempt), (11, 2));
    assert_eq!(runs[0].conclusion.as_deref(), Some("success"));
    assert!(runs[1].conclusion.is_none());
    let parsed = parse_jobs(
        r#"{"jobs": [{"name": "static (unit)", "conclusion": "failure", "steps": [
            {"name": "Set up job", "conclusion": "success"},
            {"name": "Run unit tests", "conclusion": "failure"},
            {"name": "Post", "conclusion": "failure"}]}]}"#,
    )
    .unwrap();
    assert_eq!(parsed[0].step.as_deref(), Some("Run unit tests"));
    assert!(parse_jobs("nope").is_err());
}
