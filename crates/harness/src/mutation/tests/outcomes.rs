use super::*;

#[test]
fn evaluated_floor_and_checked_arithmetic_property_table() {
    let mut checked = 0;
    for caught in [0, 1, 29, 30, 31, 38, 64, u64::MAX - 1, u64::MAX] {
        for unviable in [0, 1, 30, u64::MAX] {
            for missed in [0, 1, u64::MAX] {
                let mut sample = sample();
                sample.caught = caught;
                sample.unviable = unviable;
                sample.missed = missed;
                sample.timeout = 0;
                sample.success = 0;
                let sum = u128::from(caught) + u128::from(unviable) + u128::from(missed);
                sample.total_mutants = sum as u64;
                let verdict = assess(&sample, true).0;
                if sum > u128::from(u64::MAX) || u128::from(caught) + u128::from(missed) < 30 {
                    assert_eq!(verdict, Verdict::Inconclusive);
                } else if missed > 0 {
                    assert_eq!(verdict, Verdict::Regression);
                } else {
                    assert_eq!(verdict, Verdict::Pass);
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 100);
    for (caught, unviable, success) in [(29, 1, 0), (0, 30, 0), (29, 0, 1), (30, 0, 1)] {
        let mut sample = sample();
        sample.caught = caught;
        sample.unviable = unviable;
        sample.success = success;
        sample.total_mutants = caught + unviable + success;
        assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
    }
    let mut sample = sample();
    sample.caught = 30;
    sample.unviable = 0;
    sample.timeout = 1;
    sample.total_mutants = 31;
    assert_eq!(assess(&sample, false).0, Verdict::Regression);
    sample.caught = u64::MAX;
    sample.timeout = 1;
    sample.total_mutants = 0;
    assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
}
#[test]
fn inventory_and_baseline_are_exact_complete_unique_and_closed() {
    let (wire, inventory) = artifacts(30, 0);
    assert_eq!(
        assess(&parse(&wire, &inventory).unwrap(), true).0,
        Verdict::Pass
    );
    let mut bad = inventory.clone();
    bad.as_array_mut().unwrap().pop();
    assert!(parse(&wire, &bad).is_err());
    let mut bad = inventory.clone();
    bad[1] = bad[0].clone();
    assert!(parse(&wire, &bad).is_err());
    let mut bad = inventory.clone();
    bad[0]["replacement"] = json!("different");
    assert!(parse(&wire, &bad).is_err());
    let mut bad = inventory.clone();
    bad[0]["role"] = json!("approved");
    assert!(parse(&wire, &bad).is_err());
    let mut bad = wire.clone();
    bad["outcomes"].as_array_mut().unwrap().remove(0);
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    let duplicate = bad["outcomes"][0].clone();
    bad["outcomes"].as_array_mut().unwrap().push(duplicate);
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    bad["outcomes"][2] = bad["outcomes"][1].clone();
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    bad["outcomes"][0]["phase_results"][1]["process_status"] = json!({"Failure":1});
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    bad["outcomes"][0]["phase_results"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    bad["outcomes"][1]["phase_results"][1]["duration"] = json!(-1);
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    bad["outcomes"][1]["phase_results"][1]["approved"] = json!(true);
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    bad["cargo_mutants_version"] = json!("wrong");
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    bad["end_time"] = Value::Null;
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    bad["caught"] = json!(31);
    assert!(parse(&bad, &inventory).is_err());
    let mut bad = wire.clone();
    bad["outcomes"][1]["summary"] = json!("Caught");
    assert!(parse(&bad, &inventory).is_err());
}
#[test]
fn pinned_phase_precedence_and_missed_timeout_are_not_generic_unknowns() {
    let (wire, inventory) = artifacts(30, 0);
    for (status, summary, counter) in [
        (json!("Success"), "MissedMutant", "missed"),
        (json!("Timeout"), "Timeout", "timeout"),
    ] {
        let mut bad = wire.clone();
        bad["caught"] = json!(29);
        bad[counter] = json!(1);
        bad["outcomes"][1]["summary"] = json!(summary);
        bad["outcomes"][1]["phase_results"][1]["process_status"] = status;
        assert_eq!(
            assess(&parse(&bad, &inventory).unwrap(), false).0,
            Verdict::Regression
        );
    }
    let mut bad = wire.clone();
    bad["outcomes"][1]["phase_results"][1]["process_status"] = json!({"Signalled":15});
    assert!(parse(&bad, &inventory).is_err());
    bad["outcomes"][1]["summary"] = json!("Failure");
    assert!(parse(&bad, &inventory).is_err());
    bad["outcomes"][1]["phase_results"][1]["process_status"] = json!("Other");
    assert!(parse(&bad, &inventory).is_err());
    // Successful Check-only mutant is not evaluated Test evidence, and must not pass.
    let mut success = wire.clone();
    success["caught"] = json!(29);
    success["success"] = json!(1);
    success["outcomes"][1]["summary"] = json!("Success");
    success["outcomes"][1]["phase_results"] = json!([phase("Check", json!("Success"))]);
    assert_eq!(
        assess(&parse(&success, &inventory).unwrap(), true).0,
        Verdict::Inconclusive
    );
    let (unviable, inventory) = artifacts(30, 1);
    assert_eq!(
        assess(&parse(&unviable, &inventory).unwrap(), true).0,
        Verdict::Pass
    );
}
#[test]
fn duplicate_keys_before_value_collapse_and_collection_text_limits() {
    let (wire, inventory) = artifacts(30, 0);
    let bytes = serde_json::to_vec(&inventory).unwrap();
    let raw = serde_json::to_string(&wire).unwrap();
    for needle in ["\"caught\":30", "\"column\":1", "\"duration\":0.001"] {
        let replacement = format!("{needle},{needle}");
        let duplicate = raw.replacen(needle, &replacement, 1);
        assert_ne!(duplicate, raw);
        assert!(super::super::outcomes::parse(duplicate.as_bytes(), &bytes).is_err());
    }
    let mut oversized = inventory.clone();
    oversized[0]["diff"] = json!("D".repeat(65537));
    assert!(parse(&wire, &oversized).is_err());
    let mut oversized = wire.clone();
    oversized["outcomes"][1]["phase_results"][0]["argv"] = json!(vec!["cargo"; 129]);
    assert!(parse(&oversized, &inventory).is_err());
    assert!(super::super::outcomes::parse(&vec![b' '; 4 * 1024 * 1024 + 1], &bytes).is_err());
}
#[test]
fn descriptor_null_function_deletions_spans_and_scope_paths() {
    let (mut wire, mut inventory) = artifacts(30, 0);
    wire["outcomes"][1]["scenario"]["Mutant"]["function"] = Value::Null;
    inventory[0]["function"] = Value::Null;
    assert!(parse(&wire, &inventory).is_ok());
    // Empty insertion spans are legitimate pinned wire records.
    wire["outcomes"][1]["scenario"]["Mutant"]["span"]["end"] = json!({"line":1,"column":1});
    inventory[0]["span"]["end"] = json!({"line":1,"column":1});
    assert!(parse(&wire, &inventory).is_ok());
    for path in [
        "../source.rs",
        "crates//source.rs",
        "/source.rs",
        "crates/./source.rs",
        "unselected.rs",
    ] {
        let mut bad = inventory.clone();
        bad[0]["file"] = json!(path);
        assert!(parse(&wire, &bad).is_err());
    }
    let mut bad = inventory.clone();
    bad[0]["span"]["start"]["line"] = json!(0);
    assert!(parse(&wire, &bad).is_err());
    let mut bad = inventory.clone();
    bad[0]["span"]["end"]["line"] = json!(0);
    assert!(parse(&wire, &bad).is_err());
    let mut bad = inventory.clone();
    bad[0].as_object_mut().unwrap().remove("function");
    assert!(parse(&wire, &bad).is_err());
    let mut bad = wire.clone();
    bad["outcomes"][1]["log_path"] = json!("../log");
    assert!(parse(&bad, &inventory).is_err());
}
