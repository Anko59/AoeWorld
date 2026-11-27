use super::*;
use serde_json::json;

#[test]
fn shard_duration_regeneration_rounds_sums_attempts_and_rejects_overlapping_reports() {
    let directory = tempfile::tempdir().unwrap();
    let report = directory.path().join("report.json");
    let output = directory.path().join("durations.json");
    fs::write(&report, serde_json::to_vec(&json!({
        "errors": [], "suites": [{"specs": [
            {"file": "a.spec.ts", "title": "retry", "tests": [{"projectName": "gpu", "results": [{"duration": 750}, {"duration": 750}]}]},
            {"file": "a.spec.ts", "title": "fast", "tests": [{"projectName": "gpu", "results": [{"duration": 1499}]}]},
            {"file": "a.spec.ts", "title": "skipped", "tests": [{"projectName": "gpu", "results": [{"duration": 0}]}]}
        ]}]
    })).unwrap()).unwrap();
    let mut args = DurationsArgs {
        reports: vec![report.clone()],
        output: output.clone(),
        list: None,
    };
    regenerate(&args).unwrap();
    let weights: Durations = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(weights["[gpu] › a.spec.ts › retry"], 2);
    assert_eq!(weights["[gpu] › a.spec.ts › fast"], 1);
    assert_eq!(weights["[gpu] › a.spec.ts › skipped"], 1);
    let text = fs::read_to_string(&output).unwrap();
    // One line per file, whatever the number of tests, and the same weights as the pretty form.
    assert_eq!(text.lines().count(), 3, "{text}");
    assert!(
        text.lines().nth(1).unwrap().contains("retry")
            && text.lines().nth(1).unwrap().contains("fast")
    );
    let before = fs::read(&output).unwrap();
    args.reports.push(report);
    assert!(
        regenerate(&args)
            .unwrap_err()
            .to_string()
            .contains("overlap")
    );
    assert_eq!(before, fs::read(output).unwrap());
}
