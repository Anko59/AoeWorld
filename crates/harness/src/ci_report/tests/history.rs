use crate::ci_report::history::{Event, EventKind, Version, events, parse_micro, parse_wasm};
use serde_json::json;

fn micro(rows: &[(&str, u64)], accepted: &[(&str, u64, &str)]) -> String {
    let counts: serde_json::Map<_, _> = rows
        .iter()
        .map(|(row, count)| (format!("instructions::{row}"), json!(count)))
        .collect();
    let accepted: serde_json::Map<_, _> = accepted
        .iter()
        .map(|(row, dev, reason)| {
            (
                format!("instructions::{row}"),
                json!({"dev": dev, "reason": reason}),
            )
        })
        .collect();
    json!({
        "instructions": counts.clone(),
        "instruction_reference": counts,
        "accepted_increase": accepted,
    })
    .to_string()
}

fn version(position: usize, text: &str) -> Version {
    let mut version = parse_micro(text).expect("baseline");
    version.position = position;
    version.at = format!("2026-11-{:02}T10:00:00Z", position + 1);
    version.pr = Some(100 + position as u64);
    version.title = format!("change {position}");
    version
}

fn kinds(found: &[Event]) -> Vec<(char, EventKind, Vec<&str>)> {
    found
        .iter()
        .map(|e| {
            (
                e.id.chars().next().unwrap(),
                e.kind,
                e.rows.iter().map(String::as_str).collect(),
            )
        })
        .collect()
}

#[test]
fn a_baseline_that_moves_by_more_than_two_times_marks_a_redefinition() {
    let found = events(&[
        version(
            0,
            &micro(&[("a::up", 100), ("a::down", 1_000), ("a::calm", 100)], &[]),
        ),
        version(
            1,
            &micro(&[("a::up", 250), ("a::down", 400), ("a::calm", 150)], &[]),
        ),
    ]);
    assert_eq!(
        kinds(&found),
        vec![('A', EventKind::Redefined, vec!["a::down", "a::up"])]
    );
    assert_eq!(found[0].position, 1);
    assert_eq!(found[0].pr, Some(101));
}

#[test]
fn exactly_two_times_and_a_new_row_are_not_redefinitions() {
    let found = events(&[
        version(0, &micro(&[("a::x", 100)], &[])),
        version(1, &micro(&[("a::x", 200), ("a::new", 5_000)], &[])),
    ]);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_accepted_increase_is_marked_once_and_again_when_its_count_changes() {
    let rows = [("a::x", 100)];
    let found = events(&[
        version(0, &micro(&rows, &[])),
        version(1, &micro(&rows, &[("a::x", 110, "caps removed; accepted")])),
        version(
            2,
            &micro(&[("a::x", 105)], &[("a::x", 110, "caps removed; accepted")]),
        ),
        version(
            3,
            &micro(&[("a::x", 105)], &[("a::x", 120, "more work; accepted")]),
        ),
    ]);
    assert_eq!(
        kinds(&found),
        vec![
            ('A', EventKind::Accepted, vec!["a::x"]),
            ('B', EventKind::Accepted, vec!["a::x"]),
        ]
    );
    assert_eq!(found[0].what, "caps removed; accepted");
    assert_eq!(found[1].position, 3);
}

#[test]
fn a_long_reason_is_cut_at_its_first_sentence_or_a_length() {
    let long = format!("First sentence here. {}", "more words ".repeat(40));
    let found = events(&[
        version(0, &micro(&[("a::x", 100)], &[])),
        version(1, &micro(&[("a::x", 100)], &[("a::x", 110, &long)])),
    ]);
    assert_eq!(found[0].what, "First sentence here.");
    let found = events(&[
        version(0, &micro(&[("a::x", 100)], &[])),
        version(
            1,
            &micro(&[("a::x", 100)], &[("a::x", 110, &"w".repeat(400))]),
        ),
    ]);
    assert!(found[0].what.chars().count() <= 161, "{}", found[0].what);
}

#[test]
fn events_are_lettered_in_order_and_one_commit_merges_its_rows() {
    let found = events(&[
        version(0, &micro(&[("a::x", 100), ("a::y", 100)], &[])),
        version(1, &micro(&[("a::x", 900), ("a::y", 900)], &[])),
        version(2, &micro(&[("a::x", 100), ("a::y", 900)], &[])),
    ]);
    let ids: Vec<_> = found.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["A", "B"]);
    assert_eq!(found[0].rows.len(), 2);
    assert_eq!(found[1].rows, ["a::x"]);
}

#[test]
fn many_events_run_past_z_without_repeating_a_label() {
    let mut versions = vec![version(0, &micro(&[("a::x", 1)], &[]))];
    for step in 1..=28 {
        let count = if step % 2 == 1 { 10 } else { 1 };
        versions.push(version(step, &micro(&[("a::x", count)], &[])));
    }
    let found = events(&versions);
    let ids: std::collections::BTreeSet<_> = found.iter().map(|e| e.id.clone()).collect();
    assert_eq!(found.len(), 28);
    assert_eq!(ids.len(), 28);
    assert_eq!(found[26].id, "AA");
}

#[test]
fn the_baseline_files_parse_into_counts_limits_and_accepted_levels() {
    let text = micro(
        &[("simulation::late_game", 100)],
        &[("simulation::late_game", 90, "r")],
    );
    let parsed = parse_micro(&text).unwrap();
    assert_eq!(parsed.instructions["simulation::late_game"], 100);
    assert_eq!(parsed.reference["simulation::late_game"], 100);
    assert_eq!(parsed.accepted["simulation::late_game"].dev, 90);
    assert_eq!(
        parse_wasm(r#"{"optimized_gzip_bytes": 375952}"#).unwrap(),
        375_952
    );
    assert!(parse_micro("{}").is_err());
}
