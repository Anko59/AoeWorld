use super::*;
use crate::game_score::pr::fixtures::{late, natural, numbers_in, report, set, standard};
use serde_json::json;

fn find<'a>(diffs: &'a [ScenarioDiff], scenario: &str, window: &str) -> &'a WindowDiff {
    diffs
        .iter()
        .find(|diff| diff.id == scenario)
        .unwrap_or_else(|| panic!("no scenario {scenario}"))
        .windows
        .iter()
        .find(|w| w.id == window)
        .unwrap_or_else(|| panic!("no window {window}"))
}

fn paths(window: &WindowDiff) -> Vec<&str> {
    window.changed.iter().map(|row| row.path.as_str()).collect()
}

#[test]
fn identical_reports_change_nothing_and_count_every_number() {
    let both = standard();
    let diffs = compare(Some(&both), &both);
    assert_eq!(diffs.len(), 2);
    for diff in &diffs {
        assert_eq!(diff.status, Status::Compared, "{}", diff.id);
        for window in &diff.windows {
            assert!(window.changed.is_empty(), "{}/{}", diff.id, window.id);
            assert_eq!(
                window.unchanged,
                numbers_in(&both["scenarios"][0]["windows"][0])
            );
        }
    }
}

#[test]
fn a_changed_service_number_is_listed_with_both_values_and_its_change() {
    let base = standard();
    let mut branch = standard();
    set(
        &mut branch,
        "natural",
        "early",
        "service.arrival.arrived_permille",
        json!(940),
    );
    let diffs = compare(Some(&base), &branch);
    let early = find(&diffs, "natural", "early");
    assert_eq!(early.changed.len(), 1);
    let row = &early.changed[0];
    assert_eq!(row.path, "arrival.arrived_permille");
    assert_eq!((row.base, row.branch), (Some(971.0), Some(940.0)));
    assert!((row.percent().expect("percent") - -3.19).abs() < 0.01);
    assert_eq!(row.direction(), Direction::Worse);
    assert_eq!(row.label(), "share of orders arrived");
    let total = numbers_in(&base["scenarios"][0]["windows"][0]);
    assert_eq!(
        early.unchanged,
        total - 1,
        "the others are folded into a count"
    );
    for other in ["middle", "whole"] {
        assert!(find(&diffs, "natural", other).changed.is_empty(), "{other}");
    }
}

#[test]
fn changed_numbers_come_in_the_order_of_the_table_then_by_path() {
    let base = standard();
    let mut branch = standard();
    for (path, value) in [
        ("service.planning.approached", 4),
        ("service.arrival.arrived_permille", 900),
        ("service.size.units", 1_000),
        ("service.economy.income_faults", 1),
    ] {
        set(&mut branch, "late", "late", path, json!(value));
    }
    let diffs = compare(Some(&base), &branch);
    assert_eq!(
        paths(find(&diffs, "late", "late")),
        [
            "size.units",
            "arrival.arrived_permille",
            "economy.income_faults",
            "planning.approached"
        ]
    );
}

#[test]
fn a_number_that_gains_or_loses_a_value_changes_without_a_direction() {
    let base = standard();
    let mut branch = standard();
    set(
        &mut branch,
        "natural",
        "early",
        "service.arrival.order_to_first_step_ticks.p50",
        json!(null),
    );
    let diffs = compare(Some(&base), &branch);
    let row = &find(&diffs, "natural", "early").changed[0];
    assert_eq!((row.base, row.branch), (Some(1.0), None));
    assert_eq!(row.percent(), None);
    assert_eq!(row.direction(), Direction::Neither);
}

#[test]
fn wall_time_and_memory_sit_beside_and_never_among_the_changed_numbers() {
    let base = standard();
    let mut branch = standard();
    set(
        &mut branch,
        "natural",
        "early",
        "cost.tick_seconds",
        json!(9.0),
    );
    set(
        &mut branch,
        "natural",
        "early",
        "cost.peak_memory_mib",
        json!(500),
    );
    let diffs = compare(Some(&base), &branch);
    let early = find(&diffs, "natural", "early");
    assert!(early.changed.is_empty(), "{:?}", early.changed);
    let noisy: Vec<&str> = early.noisy.iter().map(|row| row.path.as_str()).collect();
    for expected in [
        "cost.tick_seconds",
        "cost.tick_ms.p95",
        "cost.peak_memory_mib",
    ] {
        assert!(noisy.contains(&expected), "{expected} in {noisy:?}");
    }
    assert!(
        !noisy.contains(&"cost.instructions"),
        "no instruction count on either side: no row"
    );
}

#[test]
fn a_cost_per_unit_of_service_is_judged_on_instructions_and_left_out_on_wall_time() {
    let base = standard();
    let mut branch = standard();
    set(
        &mut branch,
        "natural",
        "early",
        "efficiency.per_arrival",
        json!(0.9),
    );
    let wall = compare(Some(&base), &branch);
    let early = find(&wall, "natural", "early");
    assert!(early.changed.is_empty(), "wall time is never judged");
    assert!(
        early.noisy.iter().all(|row| row.path.starts_with("cost.")),
        "and it is the seconds divided by a count: not shown twice"
    );

    let (mut base, mut branch) = (standard(), standard());
    for report in [&mut base, &mut branch] {
        set(
            report,
            "natural",
            "early",
            "efficiency.basis",
            json!("instructions"),
        );
        set(
            report,
            "natural",
            "early",
            "cost.instructions",
            json!(5_000_000),
        );
    }
    set(
        &mut branch,
        "natural",
        "early",
        "efficiency.per_arrival",
        json!(0.9),
    );
    let counted = compare(Some(&base), &branch);
    let early = find(&counted, "natural", "early");
    assert_eq!(paths(early), ["efficiency.per_arrival"]);
    assert_eq!(early.changed[0].direction(), Direction::Worse);
    assert_eq!(early.changed[0].label(), "cost per arrival");
    assert!(
        early
            .noisy
            .iter()
            .any(|row| row.path == "cost.instructions"),
        "the count itself is shown beside"
    );
}

#[test]
fn the_noisy_rows_come_in_a_fixed_order() {
    let report = standard();
    let diffs = compare(Some(&report), &report);
    let order: Vec<&str> = find(&diffs, "late", "late")
        .noisy
        .iter()
        .map(|row| row.path.as_str())
        .collect();
    assert_eq!(
        order,
        [
            "cost.tick_seconds",
            "cost.tick_ms.p95",
            "cost.peak_memory_mib"
        ]
    );
}

#[test]
fn a_scenario_whose_definition_changed_is_not_compared() {
    let base = report(vec![natural(), late()]);
    let mut branch = report(vec![natural(), late()]);
    branch["scenarios"][0]["ticks"] = json!(8_000);
    let diffs = compare(Some(&base), &branch);
    match &diffs[0].status {
        Status::NotComparable(why) => assert!(why.contains("ticks"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        diffs[1].status,
        Status::Compared,
        "the other scenario still is"
    );
    let mut moved = report(vec![natural()]);
    moved["scenarios"][0]["windows"][0]["to_tick"] = json!(3_500);
    let diffs = compare(Some(&report(vec![natural()])), &moved);
    assert!(matches!(diffs[0].status, Status::NotComparable(_)));
}

#[test]
fn a_scenario_the_merge_base_lacks_has_no_base() {
    let base = report(vec![natural()]);
    let diffs = compare(Some(&base), &standard());
    assert_eq!(diffs[0].status, Status::Compared);
    assert_eq!(
        diffs[1].status,
        Status::NoBase("the merge base has no scenario late".to_owned())
    );
}

#[test]
fn without_a_merge_base_the_headline_numbers_of_the_branch_stand_alone() {
    let diffs = compare(None, &standard());
    for diff in &diffs {
        assert!(matches!(diff.status, Status::NoBase(_)));
        for window in &diff.windows {
            assert_eq!(window.unchanged, 0);
            assert!(!window.changed.is_empty());
            assert!(window.changed.len() < 20, "a short table, not every number");
            assert!(window.changed.iter().all(|row| row.base.is_none()));
            assert!(
                window
                    .changed
                    .iter()
                    .any(|row| row.path == "arrival.arrived_permille")
            );
        }
    }
}

#[test]
fn another_map_is_not_comparable() {
    let base = standard();
    let mut branch = standard();
    branch["header"]["map_hash"] = json!("00".repeat(32));
    for diff in compare(Some(&base), &branch) {
        assert!(
            matches!(&diff.status, Status::NotComparable(why) if why.contains("another map")),
            "{:?}",
            diff.status
        );
    }
}

#[test]
fn a_number_the_table_does_not_know_is_compared_under_its_path() {
    let mut base = standard();
    let mut branch = standard();
    for report in [&mut base, &mut branch] {
        report["scenarios"][0]["windows"][0]["service"]["fights"]["charges"] = json!(3);
    }
    branch["scenarios"][0]["windows"][0]["service"]["fights"]["charges"] = json!(5);
    let diffs = compare(Some(&base), &branch);
    let row = &find(&diffs, "natural", "early").changed[0];
    assert_eq!(row.path, "fights.charges");
    assert_eq!(row.label(), "fights.charges");
    assert_eq!(row.direction(), Direction::Neither);
}

#[test]
fn a_rise_from_zero_has_no_percent_but_a_direction() {
    let mut row = Row {
        path: "planning.stuck".to_owned(),
        base: Some(0.0),
        branch: Some(4.0),
    };
    assert_eq!((row.percent(), row.direction()), (None, Direction::Worse));
    row.path = "economy.delivered_total".to_owned();
    assert_eq!(row.direction(), Direction::Better);
    row.branch = Some(0.0);
    assert_eq!(row.direction(), Direction::Same);
}
