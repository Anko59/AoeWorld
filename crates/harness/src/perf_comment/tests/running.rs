//! The comment is posted by the last step of the job it describes, so that job has no end yet.
use crate::perf_comment::{epoch_seconds, parse_jobs_at};

#[test]
fn a_job_that_is_still_running_is_timed_up_to_now() {
    let text = r#"{"jobs": [
        {"name": "static", "conclusion": "success",
         "started_at": "2026-10-14T11:00:00Z", "completed_at": "2026-10-14T11:10:00Z"},
        {"name": "target-performance", "conclusion": null,
         "started_at": "2026-10-14T11:00:00Z", "completed_at": null},
        {"name": "queued", "conclusion": null, "started_at": null, "completed_at": null},
        {"name": "lost", "conclusion": "cancelled",
         "started_at": "2026-10-14T11:00:00Z", "completed_at": null}]}"#;
    let now = epoch_seconds("2026-10-14T11:27:05Z").unwrap();
    let jobs = parse_jobs_at(text, 7, now).unwrap();
    let seconds: Vec<_> = jobs.iter().map(|job| job.seconds).collect();
    // A finished job keeps its own end, a running one ends now, and a job
    // that never started or ended without a time has no duration.
    assert_eq!(seconds, [Some(600), Some(1_625), None, None]);
}
