use super::*;

#[test]
fn summary_reports_order_statistics() {
    let series = Series((1..=100).map(f64::from).collect());
    let summary = series.summary();
    assert_eq!(summary["max_ms"], 100.0);
    assert_eq!(summary["p50_ms"], 51.0);
    assert_eq!(summary["samples"], 100);
    assert_eq!(Series::default().summary()["max_ms"], 0.0);
}
