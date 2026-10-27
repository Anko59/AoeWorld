//! Fixture artifact directories, rendered tables and the in-place comment
//! arguments. All `gh` and download flows stay behind pure seams.
use super::reports::{self, Report, WasmRule};
use super::{comment_args, epoch_seconds, format_duration, own_comment, parse_jobs, render};
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
fn noise_config_keeps_widening_and_run_count() {
    let noise = render::Noise::load().expect("noise.json");
    assert_eq!(noise.widening_percent(), 25);
    assert_eq!(noise.dev_runs(), 5);
}

#[test]
fn the_band_is_devs_range_widened_by_a_quarter() {
    let noise = render::Noise::load().expect("noise.json");
    // A real range widens at both ends.
    assert_eq!(
        noise.band(&[1000, 900, 1100, 1000, 1000]),
        Some((850.0, 1150.0))
    );
    // Equal values have an empty range: the median widens instead.
    assert_eq!(noise.band(&[1000, 1000, 1000]), Some((750.0, 1250.0)));
    assert_eq!(noise.band(&[700, 700]), Some((525.0, 875.0)));
    assert_eq!(noise.band(&[]), None);
}

#[test]
fn timing_rows_come_from_devs_own_band() {
    let dir = tempdir().expect("directory");
    let mut devs = Vec::new();
    for (i, hotspot) in [1000u64, 900, 1100, 1000, 1000].into_iter().enumerate() {
        let path = dir.path().join(format!("dev-{i}"));
        fixture(
            &path,
            &[
                ("smoke", vec![1000; 4]),
                ("target-hotspot", vec![hotspot; 4]),
            ],
            &[("simulation::advance", 100)],
            1000,
            100,
            Some((vec![1000; 10], 5, 0)),
        );
        devs.push((100 + i as u64, read(&path)));
    }
    let pr_path = dir.path().join("pr");
    fixture(
        &pr_path,
        &[("smoke", vec![1300; 4]), ("target-hotspot", vec![1140; 4])],
        &[("simulation::advance", 100)],
        1100,
        120,
        Some((vec![2400; 10], 5, 2)),
    );
    let noise = render::Noise::load().expect("noise.json");
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 1,
        devs: &devs,
        pr: Some(&read(&pr_path)),
        jobs: &[],
        wasm: Some(WasmRule {
            baseline: 150,
            cap: 157,
        }),
        noise: &noise,
    });
    assert!(comment.starts_with(render::MARKER));
    // Deterministic rows: absolute values, the change, baseline and cap.
    assert!(
        comment.contains(
            "| Optimized WASM raw (bytes) | 1000 | 1100 | +100 (+10.0%) | deterministic |"
        )
    );
    assert!(comment.contains(
        "| Optimized WASM gzip (bytes) | 100 | 120 | +20 (+20.0%) | deterministic; baseline 150, cap 157 |"
    ));
    assert!(comment.contains(
        "| Instructions: simulation::advance | 100 | 100 | +0 (+0.0%) | deterministic |"
    ));
    // Shown: outside dev's own band. Smoke spans [750, 1250]; the pressure
    // latency and the missed deadlines leave their bands by far.
    assert!(
        comment.contains(
            "| Tick p50, smoke (ns) | 1000 | 1300 | +300 (+30.0%) | timing, noisy runner |"
        )
    );
    // The nearest-rank p95 of the same samples is their maximum, so it sits
    // outside the same band and shows too.
    assert!(
        comment.contains(
            "| Tick p95, smoke (ns) | 1000 | 1300 | +300 (+30.0%) | timing, noisy runner |"
        )
    );
    assert!(comment.contains(
        "| Pressure snapshot latency p50 (µs) | 1000 | 2400 | +1400 (+140.0%) | timing, noisy runner |"
    ));
    assert!(
        comment.contains("| Pressure missed offer deadlines | 0 | 2 | +2 | timing, noisy runner |")
    );
    // Hidden: inside dev's own band, even when they moved against the newest
    // dev run. target-hotspot spans [850, 1150], so p50 1140 and p95 1140 stay quiet.
    assert!(!comment.contains("Tick p50, target-hotspot"));
    assert!(!comment.contains("Tick p95, target-hotspot"));
    assert!(!comment.contains("max response backlog |"));
    // The band is described, and unchanged instruction counts say why the
    // shown timing differences mean nothing.
    assert!(comment.contains("across its last 5 runs whose `target-performance` job passed"));
    assert!(comment.contains("widened by 25% of that range"));
    assert!(
        comment.contains(
            "instruction counts unchanged: timing differences are most likely runner noise"
        )
    );
}

#[test]
fn fewer_than_three_dev_reports_hide_all_timings_with_the_note() {
    let dir = tempdir().expect("directory");
    let mut devs = Vec::new();
    for i in 0..2 {
        let path = dir.path().join(format!("dev-{i}"));
        fixture(
            &path,
            &[("smoke", vec![1000; 4])],
            &[("simulation::advance", 100)],
            1000,
            100,
            Some((vec![1000; 10], 5, 0)),
        );
        devs.push((10 + i as u64, read(&path)));
    }
    let pr_path = dir.path().join("pr");
    fixture(
        &pr_path,
        &[("smoke", vec![2000; 4])],
        &[("simulation::advance", 100)],
        1000,
        100,
        Some((vec![2000; 10], 5, 3)),
    );
    let noise = render::Noise::load().expect("noise.json");
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 1,
        devs: &devs,
        pr: Some(&read(&pr_path)),
        jobs: &[],
        wasm: None,
        noise: &noise,
    });
    // Deterministic rows survive …
    assert!(comment.contains(
        "| Instructions: simulation::advance | 100 | 100 | +0 (+0.0%) | deterministic |"
    ));
    // … but no timing row appears, however large the gap, and the comment
    // says why. The instruction line still holds: the measured paths did not
    // change, whatever the hidden timings did.
    assert!(!comment.contains("timing, noisy runner"));
    assert!(
        comment.contains("No timing rows: only 2 `dev` runs whose `target-performance` job passed")
    );
    assert!(comment.contains("needs at least 3"));
    assert!(
        comment.contains(
            "instruction counts unchanged: timing differences are most likely runner noise"
        )
    );
}

#[test]
fn changed_instruction_counts_silence_the_unchanged_line() {
    let dir = tempdir().expect("directory");
    let mut devs = Vec::new();
    for i in 0..3 {
        let path = dir.path().join(format!("dev-{i}"));
        fixture(
            &path,
            &[("smoke", vec![1000; 4])],
            &[("simulation::advance", 100)],
            1000,
            100,
            None,
        );
        devs.push((10 + i as u64, read(&path)));
    }
    let pr_path = dir.path().join("pr");
    fixture(
        &pr_path,
        &[("smoke", vec![1000; 4])],
        &[("simulation::advance", 101)],
        1000,
        100,
        None,
    );
    let noise = render::Noise::load().expect("noise.json");
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 1,
        devs: &devs,
        pr: Some(&read(&pr_path)),
        jobs: &[],
        wasm: None,
        noise: &noise,
    });
    assert!(comment.contains(
        "| Instructions: simulation::advance | 100 | 101 | +1 (+1.0%) | deterministic |"
    ));
    assert!(!comment.contains("instruction counts unchanged"));
}

#[test]
fn the_wasm_note_marks_only_values_over_the_cap() {
    let dir = tempdir().expect("directory");
    let mut devs = Vec::new();
    for i in 0..3 {
        let path = dir.path().join(format!("dev-{i}"));
        fixture(&path, &[("smoke", vec![1000; 4])], &[], 1000, 100, None);
        devs.push((10 + i as u64, read(&path)));
    }
    let noise = render::Noise::load().expect("noise.json");
    let at_cap = dir.path().join("at-cap");
    fixture(&at_cap, &[], &[], 1000, 157, None);
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 1,
        devs: &devs,
        pr: Some(&read(&at_cap)),
        jobs: &[],
        wasm: Some(WasmRule {
            baseline: 150,
            cap: 157,
        }),
        noise: &noise,
    });
    assert!(comment.contains(
        "| Optimized WASM gzip (bytes) | 100 | 157 | +57 (+57.0%) | deterministic; baseline 150, cap 157 |"
    ));
    assert!(!comment.contains("PR over cap"));
    let over_cap = dir.path().join("over-cap");
    fixture(&over_cap, &[], &[], 1000, 158, None);
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 1,
        devs: &devs,
        pr: Some(&read(&over_cap)),
        jobs: &[],
        wasm: Some(WasmRule {
            baseline: 150,
            cap: 157,
        }),
        noise: &noise,
    });
    assert!(comment.contains(
        "| Optimized WASM gzip (bytes) | 100 | 158 | +58 (+58.0%) | deterministic; baseline 150, cap 157; PR over cap |"
    ));
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
        devs: &[],
        pr: Some(&read(&pr)),
        jobs: &[],
        wasm: None,
        noise: &noise,
    });
    assert!(comment.contains(
        "dev` run whose `target-performance` job passed uploaded a `performance-report`"
    ));
    assert!(comment.contains("| Optimized WASM raw (bytes) | — | 10 | — | deterministic |"));
    // Without dev values nothing can move beyond the band: no timing rows.
    assert!(!comment.contains("timing, noisy runner"));
    assert!(
        comment
            .contains("| Optimized WASM gzip (bytes) | — | 10 | — | deterministic; cap unknown |")
    );
    assert!(
        comment.contains("No timing rows: only 0 `dev` runs whose `target-performance` job passed")
    );
}

#[test]
fn missing_pr_artifact_is_stated_and_job_durations_render() {
    let noise = render::Noise::load().expect("noise.json");
    let comment = render::comment(render::Input {
        repository: "o/r",
        run: 5,
        devs: &[(2, fixture_report())],
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
        wasm: None,
        noise: &noise,
    });
    assert!(comment.contains("uploaded no `performance-report` artifact"));
    assert!(comment.contains("| target-performance | 20m43s |"));
    assert!(comment.contains("| static (failure) | 1m00s |"));
    assert!(!comment.contains("native-coverage"));
    assert!(comment.contains("**Not measured here:** real frame rate"));
    assert!(!comment.contains("| Metric |"));
}

/// One fixture dev report, for inputs that only need the dev side to exist.
/// The parsed `Report` owns its data, so the temporary directory may go.
fn fixture_report() -> Report {
    let dir = tempdir().expect("directory");
    fixture(
        &dir.path().join("dev"),
        &[("smoke", vec![1000; 4])],
        &[],
        10,
        10,
        None,
    );
    read(&dir.path().join("dev"))
}

#[test]
fn the_comment_is_updated_in_place_through_its_id() {
    let marker = render::MARKER;
    // Two pages as `gh api --paginate` prints them. A person quoting the
    // marker, or the bot mentioning it mid-text, is never the comment to edit.
    let pages = format!(
        r#"[{{"id": 1, "body": "{marker} quoted", "user": {{"login": "someone"}}}},
            {{"id": 2, "body": "see {marker}", "user": {{"login": "github-actions[bot]"}}}}]
           [{{"id": 3, "body": "{marker}\n## CI", "user": {{"login": "github-actions[bot]"}}}},
            {{"id": 4, "body": "{marker}\n## CI", "user": {{"login": "github-actions[bot]"}}}}]"#
    );
    assert_eq!(own_comment(&pages).unwrap(), Some(3));
    assert_eq!(own_comment("[]").unwrap(), None);
    assert_eq!(own_comment("").unwrap(), None);
    assert!(own_comment("{").is_err());
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
fn a_report_without_the_core_file_is_refused_and_the_rule_computes_its_cap() {
    let dir = tempdir().expect("directory");
    assert!(reports::read(dir.path()).is_err());
    let cap = dir.path().join("cap.json");
    fs::write(
        &cap,
        serde_json::json!({"version": 1, "optimized_gzip_bytes": 150}).to_string(),
    )
    .expect("baseline");
    let rule = reports::wasm_rule_at(&cap).expect("rule");
    assert_eq!(rule.baseline, 150);
    // The cap is the gate's own rule: the largest value it still passes
    // (150 * 1.25 = 187.5, so 187 passes and 188 regresses).
    assert_eq!(rule.cap, 187);
    let verdict = |size| crate::perf::compare("gzip", Some(size), Some(150)).verdict;
    assert_eq!(verdict(rule.cap), crate::perf::Verdict::Pass);
    assert_eq!(verdict(rule.cap + 1), crate::perf::Verdict::Regression);
    assert_eq!(reports::wasm_rule_at(&dir.path().join("gone.json")), None);
}

#[test]
fn job_durations_use_each_jobs_own_completion() {
    // Shape of `repos/{repo}/actions/runs/38553757064/jobs`: every job ends at
    // its own time, not at the end of the run.
    let text = r#"{"total_count": 2, "jobs": [
        {"name": "select", "conclusion": "success",
         "started_at": "2026-10-14T11:02:21Z", "completed_at": "2026-10-14T11:04:17Z"},
        {"name": "static", "conclusion": "success",
         "started_at": "2026-10-14T11:04:20Z", "completed_at": "2026-10-14T11:15:14Z"}]}"#;
    let jobs = parse_jobs(text, 38553757064).unwrap();
    let seconds: Vec<_> = jobs
        .iter()
        .map(|job| (job.name.as_str(), job.seconds))
        .collect();
    assert_eq!(seconds, [("select", Some(116)), ("static", Some(654))]);
}
