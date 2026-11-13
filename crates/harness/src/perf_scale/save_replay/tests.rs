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
