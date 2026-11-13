use super::counts::summary;

#[test]
fn instruction_summary_retains_zero_samples_and_raw_units() {
    let result = summary(&[30, 0, 20, 10]);
    assert_eq!(result["samples"], 4);
    assert_eq!(result["mean"], 15.0);
    assert_eq!(result["p50"], 20);
    assert_eq!(result["p95"], 30);
    assert!(result.get("mean_ms").is_none());
}

#[test]
fn integer_percentiles_preserve_large_instruction_counts() {
    let value = (1_u64 << 54) + 1;
    assert_eq!(summary(&[value])["p95"].as_u64(), Some(value));
    assert_eq!(summary(&[])["samples"], 0);
}

#[test]
fn repeat_spreads_use_the_requested_metric_without_mixing_units() {
    let runs = [
        serde_json::json!({"instructions": {"mean": 100.0}, "cpu": {"mean_ms": 1.0}}),
        serde_json::json!({"instructions": {"mean": 102.0}, "cpu": {"mean_ms": 3.0}}),
    ];
    assert!((super::spread(&runs, "instructions", "mean").unwrap() - 200.0 / 101.0).abs() < 1e-9);
    assert_eq!(super::spread(&runs, "cpu", "mean_ms").unwrap(), 100.0);
}

#[test]
fn replay_progress_distinguishes_arrivals_motion_and_window_censoring() {
    use super::progress::Progress;
    let mut progress = Progress::default();
    progress.observe(1, (0, 0), Some((2, 0)), 0);
    progress.observe(2, (0, 1), Some((2, 1)), 0);
    progress.observe(3, (0, 2), Some((2, 2)), 0);
    progress.observe(4, (0, 3), Some((2, 3)), 0);
    progress.observe(1, (2, 0), None, 1);
    progress.observe(2, (1, 1), Some((2, 1)), 1);
    progress.observe(3, (0, 2), Some((2, 2)), 1);
    let report = progress.summary(1);
    assert_eq!(report["ordered_units"], 4);
    assert_eq!(report["arrived_units"], 1);
    assert_eq!(report["never_arrived_units"], 3);
    assert_eq!(report["zero_tile_movement_units"], 2);
    assert_eq!(report["active_at_end_units"], 2);
    assert_eq!(report["removed_units"], 1);
}
