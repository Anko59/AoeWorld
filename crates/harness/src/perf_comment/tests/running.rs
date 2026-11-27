//! The comment is posted by the last step of the job it describes, so that job has no end yet.
use crate::perf_comment::{Running, epoch_seconds, parse_jobs_at};

const JOBS: &str = r#"{"jobs": [
    {"name": "static", "conclusion": "success",
     "started_at": "2026-10-14T11:00:00Z", "completed_at": "2026-10-14T11:10:00Z"},
    {"name": "target-performance", "conclusion": null,
     "started_at": "2026-10-14T11:00:00Z", "completed_at": null},
    {"name": "e2e (1)", "conclusion": null,
     "started_at": "2026-10-14T11:02:00Z", "completed_at": null},
    {"name": "queued", "conclusion": null, "started_at": null, "completed_at": null},
    {"name": "lost", "conclusion": "cancelled",
     "started_at": "2026-10-14T11:00:00Z", "completed_at": null}]}"#;

fn running(status: &str) -> Running {
    Running {
        job: "target-performance".to_owned(),
        status: status.to_owned(),
        now: epoch_seconds("2026-10-14T11:27:05Z").unwrap(),
    }
}

#[test]
fn the_job_running_the_comment_is_timed_up_to_now_and_no_other_unfinished_job_is() {
    let jobs = parse_jobs_at(JOBS, 7, Some(&running("success"))).unwrap();
    let seconds: Vec<_> = jobs.iter().map(|job| job.seconds).collect();
    // A finished job keeps its own end; the running job ends now; another unfinished job, a job
    // that never started and a job that ended without a time have no duration.
    assert_eq!(seconds, [Some(600), Some(1_625), None, None, None]);
    // Outside CI (no running job) nothing is timed up to now.
    let jobs = parse_jobs_at(JOBS, 7, None).unwrap();
    assert_eq!(jobs[1].seconds, None);
}

#[test]
fn a_failed_step_labels_the_running_job_as_failed() {
    let status = |status: &str| {
        let jobs = parse_jobs_at(JOBS, 7, Some(&running(status))).unwrap();
        jobs[1].conclusion.clone()
    };
    assert_eq!(status("failure").as_deref(), Some("failure"));
    assert_eq!(status("success"), None);
    assert_eq!(status(""), None);
}
