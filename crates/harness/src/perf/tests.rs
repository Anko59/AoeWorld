use super::*;
use aoe_core::{Seed, Tick};

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
