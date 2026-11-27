//! Verdict tests, part two: the lines of the description that explain a move, the cost per unit
//! of service in the same game, determinism, crash and the cases with nothing to compare.
use super::tests::{has, judged, quiet, worse};
use super::*;
use crate::game_score::pr::{
    compare::compare,
    declare::parse,
    fixtures::{diverging, report, set, spread, standard},
};
use serde_json::{Value, json};

#[test]
fn a_move_the_description_explains_shows_its_reason() {
    let base = standard();
    let mut branch = standard();
    worse(&mut branch, "late", "late");
    let (declarations, unreadable) =
        parse("game-score: late arrived -7% because the escort walks with the army");
    assert!(unreadable.is_empty());
    let verdicts = judged(&base, &branch, &declarations);
    assert_eq!((verdicts.worse, verdicts.explained), (1, 1));
    assert!(
        has(
            &verdicts,
            "worse, explained: the escort walks with the army"
        ),
        "{:?}",
        verdicts.lines
    );
    assert!(
        !has(&verdicts, "a move that was intended is explained by a line"),
        "nothing left to explain"
    );
    assert!(
        verdicts.summary.contains("(1 worse, 1 explained)"),
        "{}",
        verdicts.summary
    );
}

#[test]
fn a_declaration_without_a_window_covers_every_named_window_and_one_with_a_window_only_that() {
    let base = standard();
    let mut branch = standard();
    worse(&mut branch, "natural", "early");
    worse(&mut branch, "natural", "middle");
    let (all, _) = parse("game-score: natural arrived -7% because the planner is cheaper");
    let verdicts = judged(&base, &branch, &all);
    assert_eq!((verdicts.worse, verdicts.explained), (2, 2));
    let (one, _) = parse("game-score: natural/early arrived -7% because only the early game pays");
    let verdicts = judged(&base, &branch, &one);
    assert_eq!((verdicts.worse, verdicts.explained), (2, 1));
    assert!(has(&verdicts, "natural/middle share of orders arrived"));
}

#[test]
fn a_declaration_that_understates_the_move_says_so() {
    let base = standard();
    let mut branch = standard();
    worse(&mut branch, "late", "late");
    let (declarations, _) = parse("game-score: late arrived -2% because of the new escort");
    let verdicts = judged(&base, &branch, &declarations);
    assert!(has(
        &verdicts,
        "worse, explained (declared -2%, measured -7.3%): of the new escort"
    ));
}

#[test]
fn a_declaration_that_matches_nothing_or_cannot_be_read_is_listed() {
    let base = standard();
    let (declarations, unreadable) = parse(
        "game-score: late arrived -3% because nothing moved\n\
         game-score: late arrived soon because the format is wrong",
    );
    let diffs = compare(Some(&base), &base);
    let verdicts = verdicts(&Facts {
        branch: Some(&base),
        crash: None,
        diffs: &diffs,
        declarations: &declarations,
        unreadable: &unreadable,
        spread: &spread(1.0),
    });
    assert!(has(
        &verdicts,
        "a declaration matched no number that moved beyond its usual spread: `game-score: late arrived -3% because nothing moved`"
    ));
    assert!(has(
        &verdicts,
        "could not read the declaration `game-score: late arrived soon because the format is wrong`"
    ));
}

#[test]
fn a_declaration_with_a_name_the_game_score_does_not_know_lists_the_names() {
    let base = standard();
    let (declarations, _) = parse("game-score: late arival -3% because a typo");
    let verdicts = judged(&base, &base, &declarations);
    assert!(
        has(
            &verdicts,
            "a declaration names no number the game score knows: `game-score: late arival -3% because a typo` (the names: idle, arrived, arrival-p95, first-step-p95, stuck, expired, waiting, wait-p95, delivered,"
        ),
        "{:?}",
        verdicts.lines
    );
}

fn counted(report: &mut Value) {
    set(
        report,
        "late",
        "late",
        "efficiency.basis",
        json!("instructions"),
    );
    set(
        report,
        "late",
        "late",
        "cost.instructions",
        json!(5_000_000),
    );
}

#[test]
fn in_the_same_game_a_cost_per_unit_of_service_that_grew_is_exact() {
    let (mut base, mut branch) = (standard(), standard());
    counted(&mut base);
    counted(&mut branch);
    set(
        &mut branch,
        "late",
        "late",
        "efficiency.per_arrival",
        json!(0.5700001),
    );
    let verdicts = judged(&base, &branch, &[]);
    assert!(verdicts.same_game, "no service number moved");
    assert!(
        has(
            &verdicts,
            "late/late cost per arrival: 0.57 to 0.57 (+0.0%): same game, so exact, worse"
        ),
        "{:?}",
        verdicts.lines
    );
    assert_eq!(verdicts.worse, 1);
}

#[test]
fn in_a_different_game_a_cost_per_unit_of_service_is_left_to_the_table() {
    let (mut base, mut branch) = (standard(), standard());
    counted(&mut base);
    counted(&mut branch);
    set(
        &mut branch,
        "late",
        "late",
        "efficiency.per_arrival",
        json!(0.9),
    );
    set(
        &mut branch,
        "late",
        "late",
        "service.size.units",
        json!(1_000),
    );
    let verdicts = judged(&base, &branch, &[]);
    assert!(!verdicts.same_game);
    assert_eq!(verdicts.moved, 1, "the cost is not a number of the game");
    assert!(quiet(&verdicts), "{:?}", verdicts.lines);
}

#[test]
fn runs_that_differ_between_worker_counts_are_named_with_the_first_tick() {
    let report = diverging(standard());
    let verdicts = judged(&report, &report, &[]);
    assert!(!verdicts.determinism_holds);
    assert!(
        has(
            &verdicts,
            "determinism: natural DIFFERENT GAMES at 1 worker and 15 workers, first differing tick 3209 (unit count 5 against 6)"
        ),
        "{:?}",
        verdicts.lines
    );
    assert!(
        has(&verdicts, "late at 1 worker and 15 workers"),
        "the other scenario still agrees"
    );
    assert!(
        verdicts.summary.contains("different games"),
        "{}",
        verdicts.summary
    );
}

#[test]
fn a_crash_is_a_line_with_its_cause_and_nothing_is_compared() {
    let verdicts = verdicts(&Facts {
        branch: None,
        crash: Some("the release build of this branch failed: error[E0425]"),
        diffs: &[],
        declarations: &[],
        unreadable: &[],
        spread: &spread(1.0),
    });
    assert!(verdicts.crashed);
    assert!(has(
        &verdicts,
        "crash: the release build of this branch failed: error[E0425]"
    ));
    assert!(verdicts.summary.contains("crashed"), "{}", verdicts.summary);
}

#[test]
fn a_scenario_that_cannot_be_compared_says_why_and_the_summary_says_branch_numbers_only() {
    let base = report(Vec::new());
    let branch = standard();
    let verdicts = judged(&base, &branch, &[]);
    assert!(has(
        &verdicts,
        "natural: not compared: the merge base has no scenario natural"
    ));
    assert!(!verdicts.compared && !verdicts.same_game);
    assert!(
        !verdicts
            .lines
            .iter()
            .any(|l| l.contains("same game**") || l.contains("different game**")),
        "no case when nothing was compared: {:?}",
        verdicts.lines
    );
    assert!(
        verdicts
            .summary
            .starts_with("informational: branch numbers only ("),
        "{}",
        verdicts.summary
    );
}

#[test]
fn the_summary_never_passes_what_a_status_description_holds() {
    let mut branch = standard();
    worse(&mut branch, "late", "late");
    let verdicts = judged(&standard(), &branch, &[]);
    assert!(verdicts.summary.chars().count() <= 140);
}
