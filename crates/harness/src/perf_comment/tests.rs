//! Fixture artifact directories, rendered tables and the in-place comment
//! arguments. All `gh` and download flows stay behind pure seams.
use super::reports::{self, Report};
use super::{comment_args, comment_id, epoch_seconds, format_duration, render};
use std::{fs, path::Path};
use tempfile::tempdir;

/// One artifact directory: `ci.json` plus the optional sibling files.
fn fixture(
    dir: &Path,
    ticks: &[(&str, Vec<u64>)],
    kernels: &[(&str, u64)],
    wasm_raw: usize,
    wasm_gzip: usize,
    pressure: Option<(Vec<u64>, u64, u64)>,
) {
    fs::create_dir_all(dir).expect("directory");
    let workloads: Vec<_> = ticks
        .iter()
        .map(|(name, samples)| serde_json::json!({"scenario": name, "tick_duration_ns": samples}))
        .collect();
    let comparisons: Vec<_> = kernels
        .iter()
        .map(|(kernel, count)| {
            serde_json::json!({
                "metric": format!("instructions::{kernel}"),
                "observed": count,
                "baseline": count,
                "verdict": "PASS"
            })
        })
        .collect();
    fs::write(
        dir.join("ci.json"),
        serde_json::json!({
            "version": 1,
            "revision": "a".repeat(40),
            "dirty": false,
            "workloads": workloads,
            "comparisons": comparisons
        })
        .to_string(),
    )
    .expect("ci.json");
    fs::write(dir.join("optimized.wasm"), vec![0u8; wasm_raw]).expect("wasm");
    fs::write(dir.join("optimized.wasm.gz"), vec![0u8; wasm_gzip]).expect("wasm.gz");
    if let Some((latency, backlog, missed)) = pressure {
        fs::write(
            dir.join("pressure.json"),
            serde_json::json!({
                "snapshot_latency_us": latency,
                "max_response_backlog": backlog,
                "missed_offer_deadlines": missed
            })
            .to_string(),
        )
        .expect("pressure.json");
    }
}

fn read(dir: &Path) -> Report {
    reports::read(dir).expect("report")
}

#[test]
fn percentiles_are_nearest_rank() {
    assert_eq!(reports::percentile(&[4, 1, 3, 2], 50), Some(2));
    assert_eq!(reports::percentile(&[4, 1, 3, 2], 95), Some(4));
    assert_eq!(reports::percentile(&[], 50), None);
}

#[test]
fn noise_band_is_ten_percent_from_the_committed_config() {
    let noise = render::Noise::load().expect("noise.json");
    assert_eq!(noise.percent(), 10);
    assert!(!noise.moved(Some(100), Some(110)));
    assert!(noise.moved(Some(100), Some(111)));
    assert!(!noise.moved(Some(100), Some(90)));
    assert!(noise.moved(Some(100), Some(89)));
    assert!(noise.moved(Some(0), Some(1)));
    assert!(!noise.moved(Some(5), Some(5)));
    assert!(!noise.moved(None, Some(5)));
}

#[test]
fn deterministic_rows_always_and_timing_only_beyond_the_band() {
    let dir = tempdir().expect("directory");
    let dev = dir.path().join("dev");
    let pr = dir.path().join("pr");
    fixture(
        &dev,
        &[
            ("smoke", vec![1000, 1000, 1000, 1000]),
            ("target-hotspot", vec![1000, 1000, 1000, 1000]),
        ],
        &[("simulation::advance", 100)],
        1000,
        100,
        Some((vec![1000; 10], 5, 0)),
    );
    fixture(
        &pr,
        &[
            ("smoke", vec![1050, 1050, 1050, 1050]),
            ("target-hotspot", vec![800, 800, 800, 800]),
        ],
        &[("simulation::advance", 100)],
        1100,
        120,
        Some((vec![2000; 10], 5, 1)),
    );
    let noise = render::Noise::load().expect("noise.json");
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 1,
        dev_run: Some(2),
        dev: Some(&read(&dev)),
        pr: Some(&read(&pr)),
        jobs: &[],
        cap: Some(150),
        noise: &noise,
    });
    assert!(comment.starts_with(render::MARKER));
    // Deterministic rows: absolute values, the change, and the cap.
    assert!(
        comment.contains(
            "| Optimized WASM raw (bytes) | 1000 | 1100 | +100 (+10.0%) | deterministic |"
        )
    );
    assert!(comment.contains(
        "| Optimized WASM gzip (bytes) | 100 | 120 | +20 (+20.0%) | deterministic; cap 150 |"
    ));
    assert!(comment.contains(
        "| Instructions: simulation::advance | 100 | 100 | +0 (+0.0%) | deterministic |"
    ));
    // Timing rows: −20% and +100% show, +5% and the unchanged backlog hide.
    assert!(comment.contains(
        "| Tick p50, target-hotspot (ns) | 1000 | 800 | −200 (−20.0%) | timing, noisy runner |"
    ));
    assert!(comment.contains(
        "| Pressure snapshot latency p50 (µs) | 1000 | 2000 | +1000 (+100.0%) | timing, noisy runner |"
    ));
    assert!(
        comment.contains("| Pressure missed offer deadlines | 0 | 1 | +1 | timing, noisy runner |")
    );
    assert!(!comment.contains("Tick p50, smoke"));
    assert!(!comment.contains("max response backlog |"));
}

#[test]
fn missing_dev_report_says_so_instead_of_failing() {
    let dir = tempdir().expect("directory");
    let pr = dir.path().join("pr");
    fixture(&pr, &[("smoke", vec![1000; 4])], &[("k", 7)], 10, 10, None);
    let noise = render::Noise::load().expect("noise.json");
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 1,
        dev_run: None,
        dev: None,
        pr: Some(&read(&pr)),
        jobs: &[],
        cap: None,
        noise: &noise,
    });
    assert!(comment.contains("dev` run uploaded a `performance-report`"));
    assert!(comment.contains("| Optimized WASM raw (bytes) | — | 10 | — | deterministic |"));
    // Without dev values nothing can move beyond the band: no timing rows.
    assert!(!comment.contains("timing, noisy runner"));
    assert!(
        comment
            .contains("| Optimized WASM gzip (bytes) | — | 10 | — | deterministic; cap unknown |")
    );
}

#[test]
fn missing_pr_artifact_is_stated_and_job_durations_render() {
    let noise = render::Noise::load().expect("noise.json");
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 5,
        dev_run: Some(2),
        dev: None,
        pr: None,
        jobs: &[
            super::Job {
                name: "target-performance".to_owned(),
                conclusion: Some("success".to_owned()),
                seconds: Some(1243),
            },
            super::Job {
                name: "static".to_owned(),
                conclusion: Some("failure".to_owned()),
                seconds: Some(60),
            },
            super::Job {
                name: "native-coverage".to_owned(),
                conclusion: Some("skipped".to_owned()),
                seconds: Some(5),
            },
        ],
        cap: None,
        noise: &noise,
    });
    assert!(comment.contains("uploaded no `performance-report` artifact"));
    assert!(comment.contains("| target-performance | 20m43s |"));
    assert!(comment.contains("| static (failure) | 1m00s |"));
    assert!(!comment.contains("native-coverage"));
    assert!(comment.contains("**Not measured here:** real frame rate"));
    assert!(!comment.contains("| Metric |"));
}

#[test]
fn the_comment_is_updated_in_place_through_its_id() {
    assert_eq!(comment_id("123\n"), Some(123));
    assert_eq!(comment_id("123\n456\n"), Some(123));
    assert_eq!(comment_id(""), None);
    let body = Path::new("/scratch/comment.md");
    assert_eq!(
        comment_args(Some(42), "o/r", 7, body),
        vec![
            "api".to_owned(),
            "--method".to_owned(),
            "PATCH".to_owned(),
            "repos/o/r/issues/comments/42".to_owned(),
            "-F".to_owned(),
            "body=@/scratch/comment.md".to_owned(),
        ]
    );
    assert_eq!(
        comment_args(None, "o/r", 7, body),
        vec![
            "pr".to_owned(),
            "comment".to_owned(),
            "7".to_owned(),
            "--repo".to_owned(),
            "o/r".to_owned(),
            "--body-file".to_owned(),
            "/scratch/comment.md".to_owned(),
        ]
    );
}

#[test]
fn run_job_wall_time_uses_timestamps_across_fields() {
    assert_eq!(epoch_seconds("2026-10-14T11:02:21Z"), Some(1_791_975_741));
    assert_eq!(epoch_seconds("2026-10-14T11:23:04Z"), Some(1_791_976_984));
    assert_eq!(epoch_seconds("2026-10-14T11:02:21"), None);
    assert_eq!(epoch_seconds("nope"), None);
    assert_eq!(format_duration(1243), "20m43s");
    assert_eq!(format_duration(60), "1m00s");
    assert_eq!(format_duration(45), "45s");
    assert_eq!(format_duration(7_200), "2h00m");
}

#[test]
fn a_report_without_the_core_file_is_refused() {
    let dir = tempdir().expect("directory");
    assert!(reports::read(dir.path()).is_err());
    let cap = dir.path().join("cap.json");
    fs::write(
        &cap,
        serde_json::json!({"version": 1, "optimized_gzip_bytes": 150}).to_string(),
    )
    .expect("baseline");
    assert_eq!(reports::wasm_cap_at(&cap), Some(150));
    assert_eq!(reports::wasm_cap_at(&dir.path().join("gone.json")), None);
}
