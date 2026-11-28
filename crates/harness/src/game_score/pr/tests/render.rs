use super::*;
use crate::game_score::pr::{
    compare::compare,
    fixtures::{MAP, report, scenario, set, spread, standard, window},
    verdict::{Facts, verdicts},
};
use serde_json::json;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn hit() -> Source {
    Source::Cached {
        key: "ab".repeat(32),
        age_seconds: 3 * 3600,
        revision: "1".repeat(40),
        play_seconds: 200.0,
        lookup_seconds: 0.2,
    }
}

fn miss() -> Source {
    Source::Played {
        build_seconds: 143.0,
        play_seconds: 71.0,
        key: Some("cd".repeat(32)),
    }
}

/// The section and the terminal text of `branch` against `base`.
fn pages(base: Option<&Value>, branch: &Value, base_source: &Source) -> (String, String) {
    let diffs = compare(base, branch);
    let verdicts = verdicts(&Facts {
        branch: Some(branch),
        crash: None,
        diffs: &diffs,
        declarations: &[],
        unreadable: &[],
        spread: &spread(1.0),
    });
    let page = Page {
        head: HEAD,
        base: Some(BASE),
        map_hash: MAP,
        branch: Some(branch),
        base_report: base,
        diffs: &diffs,
        verdicts: &verdicts,
        notes: &["a note".to_owned()],
        branch_source: &miss(),
        base_source,
    };
    (section(&page), terminal(&page))
}

fn changed_branch() -> Value {
    let mut branch = standard();
    set(
        &mut branch,
        "natural",
        "early",
        "service.arrival.arrived_permille",
        json!(940),
    );
    branch
}

#[test]
fn the_comment_starts_with_the_marker_and_a_hidden_line_naming_both_commits() {
    let (section, _) = pages(Some(&standard()), &changed_branch(), &hit());
    let mut lines = section.lines();
    assert_eq!(lines.next(), Some(MARKER));
    let hidden = lines.next().expect("hidden line");
    let json = hidden
        .strip_prefix("<!-- ")
        .and_then(|rest| rest.strip_suffix(" -->"))
        .expect("a hidden comment");
    let parsed: Value = serde_json::from_str(json).expect("json");
    assert_eq!(
        (parsed["head"].as_str(), parsed["base"].as_str()),
        (Some(HEAD), Some(BASE))
    );
    assert_eq!(parsed["schema"], 1);
    assert!(section.contains("## Game score (informational)"));
    assert!(section.contains("Nothing here gates the pull request."));
}

#[test]
fn the_numbers_the_table_does_not_name_are_collapsed_under_the_ones_it_does() {
    let mut branch = changed_branch();
    set(
        &mut branch,
        "natural",
        "early",
        "service.planning.approached",
        json!(4),
    );
    set(
        &mut branch,
        "natural",
        "early",
        "service.economy.income_faults",
        json!(1),
    );
    let (section, terminal) = pages(Some(&standard()), &branch, &hit());
    let named = section
        .find("| early | share of orders arrived |")
        .expect("named row");
    let details = section
        .find("<details><summary>2 more numbers changed")
        .expect("the tail is collapsed");
    let tail = section
        .find("| early | planning.approached | 0 | 4 | new |")
        .expect("tail row");
    assert!(named < details && details < tail, "{section}");
    assert!(
        terminal.contains("planning.approached"),
        "the terminal shows every number: {terminal}"
    );
    let one = pages(
        Some(&standard()),
        &{
            let mut one = standard();
            set(
                &mut one,
                "natural",
                "early",
                "service.planning.approached",
                json!(4),
            );
            one
        },
        &hit(),
    )
    .0;
    assert!(
        one.contains("<details><summary>1 more number changed"),
        "{one}"
    );
}

#[test]
fn the_comment_carries_the_whole_report_in_a_collapsed_json_block() {
    let (section, _) = pages(Some(&standard()), &changed_branch(), &hit());
    let start = section.find("```json\n").expect("a json block") + "```json\n".len();
    let end = section[start..].find("\n```").expect("closed") + start;
    let parsed: Value = serde_json::from_str(&section[start..end]).expect("valid json");
    assert_eq!(parsed["head"], HEAD);
    let natural = &parsed["scenarios"][0];
    assert_eq!(natural["id"], "natural");
    assert_eq!(
        natural["branch"]["windows"][0]["service"]["arrival"]["arrived_permille"],
        940
    );
    assert_eq!(
        natural["base"]["windows"][0]["service"]["arrival"]["arrived_permille"],
        971
    );
    assert_eq!(
        natural["branch"]["resources_delivered_per_game_minute"],
        json!([100, 250])
    );
    assert!(section.contains("<details><summary>Full report (JSON)</summary>"));
}

fn heavy() -> Value {
    let windows = (0..20)
        .map(|index| window(&format!("w{index}"), index, index + 1))
        .collect();
    report(vec![scenario("natural", 100, windows)])
}

#[test]
fn a_comment_too_long_for_github_drops_the_merge_bases_side_then_the_report() {
    let heavy = heavy();
    let (section, _) = pages(Some(&heavy), &heavy, &hit());
    assert!(
        section.chars().count() <= 60_000,
        "{}",
        section.chars().count()
    );
    assert!(section.contains("```json"));
    let start = section.find("```json\n").expect("block") + 8;
    let end = section[start..].find("\n```").expect("end") + start;
    let parsed: Value = serde_json::from_str(&section[start..end]).expect("json");
    assert!(
        parsed["scenarios"][0]["base"].is_null(),
        "the base's side went first"
    );
    assert!(!parsed["scenarios"][0]["branch"].is_null());

    let mut vast = heavy.clone();
    for index in 0..20 {
        vast["scenarios"][0]["windows"][index]["service"]["management"]["orders_by_issuer"]["padding"] =
            json!({"text": "x".repeat(3_000)});
    }
    let (section, _) = pages(Some(&vast), &vast, &hit());
    assert!(section.chars().count() <= 60_000);
    assert!(!section.contains("```json"));
    assert!(section.contains("too long for a comment"));
}

#[test]
fn without_a_merge_base_the_branch_stands_alone_and_says_why() {
    let (section, terminal) = pages(None, &standard(), &Source::Unavailable("no map".to_owned()));
    assert!(
        section.contains(
            "Not compared (no numbers for the merge base); the numbers of this branch alone:"
        ),
        "{section}"
    );
    assert!(
        section.contains("| late | share of orders arrived | - | 97.1% | - |"),
        "{section}"
    );
    assert!(terminal.contains("not compared (no numbers for the merge base); this branch alone:"));
}

#[test]
fn the_timing_list_ends_with_the_place_ship_writes_what_the_check_added() {
    let (section, terminal) = pages(Some(&standard()), &changed_branch(), &hit());
    assert!(
        section.contains(&format!("\n- this branch: built in 143 s, played in 71 s, stored as entry cdcdcdcdcdcd\n{ADDED}\n")),
        "{section}"
    );
    assert!(
        !terminal.contains(ADDED),
        "the terminal has no place for it"
    );
}

#[test]
fn a_bar_in_a_cell_cannot_break_the_table() {
    assert_eq!(cell("a | b\nc"), "a \\| b c");
}
