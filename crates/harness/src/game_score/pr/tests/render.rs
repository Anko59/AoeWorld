use super::*;
use crate::game_score::pr::{
    compare::compare,
    fixtures::{MAP, late, natural, numbers_in, report, scenario, set, spread, standard, window},
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
fn a_changed_number_is_one_row_of_the_scenarios_table() {
    let (section, _) = pages(Some(&standard()), &changed_branch(), &hit());
    assert!(
        section.contains("### natural, 100 seats, 10000 ticks"),
        "{section}"
    );
    assert!(
        section.contains("| window | number | merge base | this branch | change |\n|---|---|---|---|---|\n| early | share of orders arrived | 97.1% | 94.0% | -3.2% |\n"),
        "{section}"
    );
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
fn numbers_that_did_not_change_are_folded_into_a_count() {
    let total = numbers_in(&natural()["windows"][0]);
    let (section, terminal) = pages(Some(&standard()), &changed_branch(), &hit());
    let expected = format!(
        "{} numbers unchanged (early {}, middle {total}, whole {total}).",
        total * 3 - 1,
        total - 1
    );
    assert!(section.contains(&expected), "{section}");
    assert!(terminal.contains(&expected), "{terminal}");
}

#[test]
fn a_scenario_where_nothing_changed_says_so_instead_of_an_empty_table() {
    let (section, _) = pages(Some(&standard()), &changed_branch(), &hit());
    assert!(
        section.contains("no number changed: this branch plays the same game as the merge base"),
        "late did not change: {section}"
    );
    assert_eq!(
        section.matches("| window | number |").count(),
        1,
        "one table, natural's"
    );
}

#[test]
fn wall_time_and_memory_are_labelled_noisy_and_collapsed() {
    let mut branch = changed_branch();
    set(
        &mut branch,
        "natural",
        "early",
        "cost.tick_seconds",
        json!(9.0),
    );
    let (section, terminal) = pages(Some(&standard()), &branch, &hit());
    assert!(section.contains("<summary>Wall time and memory (noisy, never judged)</summary>"));
    assert!(
        section.contains("| early | seconds inside the ticks | 6.7 | 9 | +33% |"),
        "{section}"
    );
    assert!(
        terminal.contains("wall time and memory (noisy, never judged):"),
        "{terminal}"
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
fn the_terminal_shows_the_verdict_the_aligned_table_and_what_each_side_cost() {
    let (_, terminal) = pages(Some(&standard()), &changed_branch(), &hit());
    assert!(terminal.starts_with("game score (informational, nothing is gated): merge base bbbbbbbbbbbb against this branch aaaaaaaaaaaa on map ce1c8d7e\n"), "{terminal}");
    assert!(terminal.contains("  crash: none\n"));
    assert!(terminal.contains("  note: a note\n"));
    assert!(
        terminal.contains("  window  number                   merge base  this branch  change\n"),
        "{terminal}"
    );
    assert!(
        terminal.contains("  early   share of orders arrived       97.1%        94.0%   -3.2%\n"),
        "{terminal}"
    );
    assert!(terminal.contains("merge base: cache hit in 0.2 s (entry abababababab, made from 111111111111 3 h ago; it took 200 s to play)\n"), "{terminal}");
    assert!(
        terminal.contains(
            "this branch: built in 143 s, played in 71 s, stored as entry cdcdcdcdcdcd\n"
        ),
        "{terminal}"
    );
}

#[test]
fn a_miss_without_a_build_and_an_unavailable_base_read_plainly() {
    let played = Source::Played {
        build_seconds: 0.0,
        play_seconds: 62.0,
        key: None,
    };
    assert_eq!(
        played.line("this branch", false),
        "this branch: played in 62 s"
    );
    assert_eq!(
        played.line("merge base", true),
        "merge base: cache miss, played in 62 s"
    );
    assert_eq!(
        miss().line("merge base", true),
        "merge base: cache miss, built in 143 s, played in 71 s, stored as entry cdcdcdcdcdcd"
    );
    let gone = Source::Unavailable("it did not build".to_owned());
    assert_eq!(
        gone.line("merge base", true),
        "merge base: not available (it did not build)"
    );
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
fn the_first_verdict_line_says_which_case_it_is_and_the_terminal_has_no_markdown_in_it() {
    let (section, terminal) = pages(Some(&standard()), &standard(), &hit());
    assert!(
        section.contains("**Verdict**\n\n- **same game**: every number of every compared window"),
        "{section}"
    );
    assert!(
        terminal.contains("  same game: every number of every compared window"),
        "{terminal}"
    );
    assert!(!terminal.contains("**"), "{terminal}");
    let (section, _) = pages(Some(&standard()), &changed_branch(), &hit());
    assert!(
        section.contains("- **different game**: 1 numbers moved, 0 beyond their usual spread"),
        "{section}"
    );
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

#[test]
fn a_late_only_report_renders_too() {
    let only = report(vec![late()]);
    let (section, _) = pages(Some(&only), &only, &hit());
    assert!(section.contains("### late, 100 seats, 1200 ticks"));
}
