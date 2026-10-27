use super::*;
use aoe_core::{Seed, Tick};

#[test]
fn comparison_summary_identifies_regressions_and_missing_measurements() {
    let comparisons = [
        compare("optimized_gzip_wasm_bytes", Some(302_106), Some(287_702)),
        compare("instructions::advance", None, Some(712_509)),
        compare("new_metric", Some(10), None),
        compare("allocation_count", Some(0), Some(0)),
    ];
    let text = comparison_summary(&comparisons);
    assert!(text.contains("| Metric | Observed | Baseline | Allowance | Verdict |"));
    for row in [
        "| optimized_gzip_wasm_bytes | 302106 | 287702 | 5% | Regression |",
        "| instructions::advance | unavailable | 712509 | 5% | Inconclusive |",
        "| new_metric | 10 | unavailable | 5% | Unbaselined |",
        "| allocation_count | 0 | 0 | 5% | Pass |",
    ] {
        assert!(text.contains(row), "missing {row} in {text}");
    }
}

#[test]
fn comparator_rejects_missing_and_regressed_samples() {
    assert_eq!(network::initial_client_result(37).bytes, 37);
    assert_eq!(compare("x", None, Some(100)).verdict, Verdict::Inconclusive);
    assert_eq!(compare("x", Some(100), None).verdict, Verdict::Unbaselined);
    assert_eq!(compare("x", Some(105), Some(100)).verdict, Verdict::Pass);
    assert_eq!(compare("x", Some(0), Some(0)).verdict, Verdict::Pass);
    assert_eq!(compare("x", Some(1), Some(0)).verdict, Verdict::Regression);
    assert_eq!(
        compare("x", Some(106), Some(100)).verdict,
        Verdict::Regression
    );
    assert_eq!(
        compare("x", Some(u64::MAX), Some(1)).verdict,
        Verdict::Regression
    );
}

#[test]
fn regression_floor_is_the_smallest_regressing_value() {
    for percent in [THRESHOLD_PERCENT, crate::perf_size::JUMP_PERCENT] {
        for baseline in [0u64, 1, 19, 20, 21, 100, 105, 1_000, 261_677, 1 << 40] {
            let floor = regression_floor(baseline, percent);
            assert_eq!(
                compare_at("x", Some(floor), Some(baseline), percent).verdict,
                Verdict::Regression,
                "floor of {baseline} at {percent}%"
            );
            assert_ne!(
                compare_at("x", Some(floor - 1), Some(baseline), percent).verdict,
                Verdict::Regression,
                "below the floor of {baseline} at {percent}%"
            );
        }
    }
}

#[test]
fn comparator_cartesian_samples_preserve_verdict_and_evidence_fields() {
    let samples = [
        None,
        Some(0),
        Some(1),
        Some(19),
        Some(20),
        Some(21),
        Some(100),
        Some(105),
        Some(106),
        Some(u64::MAX - 1),
        Some(u64::MAX),
    ];
    for observed in samples {
        for baseline in samples {
            let expected = match (observed, baseline) {
                (None, _) => Verdict::Inconclusive,
                (Some(_), None) => Verdict::Unbaselined,
                (Some(actual), Some(reference)) => {
                    // Inclusive rational limit, computed without truncating the
                    // percentage or overflowing either extreme u64 sample.
                    let limit = u128::from(reference) + u128::from(reference) / 20;
                    if u128::from(actual) <= limit {
                        Verdict::Pass
                    } else {
                        Verdict::Regression
                    }
                }
            };
            let comparison = compare("instructions::tiny", observed, baseline);
            assert_eq!(comparison.verdict, expected, "{observed:?}/{baseline:?}");
            assert_eq!(comparison.metric, "instructions::tiny");
            assert_eq!(comparison.observed, observed);
            assert_eq!(comparison.baseline, baseline);
            assert_eq!(comparison.threshold_percent, 5);
        }
    }
}

#[test]
fn offline_uses_actual_scenario_dimensions_and_seed_without_path_work() {
    for seed in [Seed(23), Seed(29)] {
        let scenario = Scenario {
            world_size: 769,
            active_extent: 512,
            players: 2,
            entities: 8,
            hotspot_entities: 0,
            seed,
            ..SMOKE
        };
        let expected = WorldConfig {
            width_tiles: 769,
            height_tiles: 769,
            seed,
            ..WorldConfig::default()
        };
        let result = offline(scenario).expect("tiny actual offline workload");
        assert_eq!(result.world.config(), expected);
        assert_eq!(result.world.unit_count(), 8);
        assert_eq!(result.world.tick(), Tick(4));
        assert_eq!(result.tick_duration_ns.len(), 4);
    }
}

#[test]
fn wasm_size_reports_growth_and_fails_only_on_a_jump_over_a_quarter() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("wasm.json");
    let artifact = dir.path().join("optimized.wasm.gz");
    std::fs::write(
        &baseline,
        r#"{"version":1,"toolchain":"rustc 1.99.1","wasm_bindgen":"0.2.128","binaryen":"108","gzip":"1.12 -n -9","optimized_gzip_bytes":1000}"#,
    )
    .unwrap();
    for (size, passes) in [(1051, true), (1250, true), (1251, false)] {
        std::fs::write(&artifact, vec![7u8; size]).unwrap();
        let result = crate::perf_size::comparison_at(&baseline, &artifact).unwrap();
        assert_eq!(crate::perf_size::verdict(&result).is_ok(), passes, "{size}");
    }
    let message = crate::perf_size::verdict(&compare_at("w", Some(1251), Some(1000), 25))
        .unwrap_err()
        .to_string();
    assert!(message.contains("1 bytes over"), "{message}");
}

#[test]
fn late_game_report_rejects_a_collapsed_workload_and_preserves_phases() {
    let empty = late_game::Census::default();
    assert!(empty.validate().is_err());
    let mut result = late_game::Census {
        players: 100,
        units: 14_000,
        gathering: 6_000,
        returning: 1_000,
        moving: 6_000,
        idle_army: 4_000,
        marching_army: 4_000,
        queues: 200,
        queued: 1_000,
        trained_units: 0,
        research: 100,
        wildlife: 1_500,
        deposited: 1,
        displaced: 4_000,
        exploring: 100,
        formation_members: 1_200,
        garrisoned: 100,
        monks: 100,
        naval: 200,
        mature_queues: 25,
    };
    result.validate().unwrap();
    result.queued = 999;
    assert!(
        result.validate().is_err(),
        "endpoints must have full queues"
    );
    result.validate_activity().unwrap();
    result.queued = 799;
    assert!(
        result.validate_activity().is_err(),
        "a refill vacancy cannot hide drained queues"
    );
    result.queued = 1_000;
    let mut after = late_game::Census {
        deposited: 2,
        queued: 1_000,
        trained_units: 1,
        research: 0,
        ..result
    };
    after.validate_after(&result).unwrap();
    after.deposited = 1;
    assert!(after.validate_after(&result).is_err());
    after.deposited = 2;
    after.trained_units = result.trained_units;
    assert!(after.validate_after(&result).is_err());
    result.exploring = 0;
    assert!(result.validate().is_err());
    result.exploring = 100;
    result.displaced = 0;
    assert!(result.validate().is_err());
    result.displaced = 4_000;
    result.mature_queues = 0;
    assert!(result.validate().is_err());
    result.mature_queues = 25;
    result.gathering = 0;
    assert!(result.validate().is_err());
    let series = late_game::Series(vec![10, 1, 5, 2, 3, 8, 7, 6, 4, 9]);
    assert_eq!(series.percentile(50), 5);
    assert_eq!(series.percentile(95), 10);
}

#[test]
fn phase_series_retains_raw_samples_and_explicit_percentiles() {
    let series = late_game::Series(vec![10, 1, 5, 2, 3, 8, 7, 6, 4, 9]);
    let value = serde_json::to_value(series).unwrap();
    assert_eq!(
        value["samples_ns"],
        serde_json::json!([10, 1, 5, 2, 3, 8, 7, 6, 4, 9])
    );
    assert_eq!(value["p50_ns"], 5);
    assert_eq!(value["p95_ns"], 10);
}
