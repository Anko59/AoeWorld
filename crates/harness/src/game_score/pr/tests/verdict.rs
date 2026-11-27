use super::*;
use crate::game_score::pr::{
    compare::compare,
    declare::Declaration,
    fixtures::{set, spread, standard, unmeasured, with_entry},
    spread::Entry,
};
use serde_json::{Value, json};

/// The branch against the base, with every judged number measured at a spread of 1 percent (a
/// tolerance of 5 percent) unless `spread` says otherwise.
pub(super) fn judged_with(
    base: &Value,
    branch: &Value,
    declarations: &[Declaration],
    spread: &Spread,
) -> Verdicts {
    let diffs = compare(Some(base), branch);
    verdicts(&Facts {
        branch: Some(branch),
        crash: None,
        diffs: &diffs,
        declarations,
        unreadable: &[],
        spread,
    })
}

pub(super) fn judged(base: &Value, branch: &Value, declarations: &[Declaration]) -> Verdicts {
    judged_with(base, branch, declarations, &spread(1.0))
}

pub(super) fn worse(branch: &mut Value, scenario: &str, window: &str) {
    set(
        branch,
        scenario,
        window,
        "service.arrival.arrived_permille",
        json!(900),
    );
}

pub(super) fn has(verdicts: &Verdicts, needle: &str) -> bool {
    verdicts.lines.iter().any(|line| line.contains(needle))
}

pub(super) fn quiet(verdicts: &Verdicts) -> bool {
    !verdicts
        .lines
        .iter()
        .any(|line| line.contains("moved beyond") || line.contains("so exact"))
}

#[test]
fn the_same_game_is_said_first_and_nothing_else_is_judged() {
    let same = standard();
    let verdicts = judged(&same, &same, &[]);
    assert!(verdicts.same_game && verdicts.determinism_holds && !verdicts.crashed);
    assert!(
        verdicts.lines[0].starts_with("**same game**: every number of every compared window"),
        "{:?}",
        verdicts.lines
    );
    assert!(verdicts.lines.iter().any(|l| l == "crash: none"));
    assert!(
        has(
            &verdicts,
            "determinism: the same game, hash after hash and order after order: natural at 1 worker and 15 workers; late at 1 worker and 15 workers"
        ),
        "{:?}",
        verdicts.lines
    );
    assert_eq!((verdicts.worse, verdicts.moved), (0, 0));
    assert!(
        verdicts
            .summary
            .starts_with("informational: same game as the merge base ("),
        "{}",
        verdicts.summary
    );
}

#[test]
fn a_different_game_says_how_many_numbers_moved_and_how_many_beyond_their_usual_spread() {
    let base = standard();
    let mut branch = standard();
    worse(&mut branch, "late", "late");
    set(
        &mut branch,
        "natural",
        "early",
        "service.economy.delivered_total",
        json!(43_000),
    );
    let verdicts = judged(&base, &branch, &[]);
    assert!(!verdicts.same_game);
    assert_eq!(
        verdicts.lines[0],
        "**different game**: 2 numbers moved, 1 beyond their usual spread (one seed per scenario; the spread is measured across seeds on `dev`, docs/scale/game-score.md)"
    );
    assert_eq!(
        (
            verdicts.moved,
            verdicts.worse,
            verdicts.improved,
            verdicts.explained
        ),
        (2, 1, 0, 0)
    );
    assert_eq!(
        verdicts.summary,
        "informational: different game: 2 numbers moved, 1 beyond their usual spread (1 worse, 0 explained)"
    );
}

#[test]
fn a_judged_number_beyond_its_spread_gets_a_line_in_the_new_words() {
    let base = standard();
    let mut branch = standard();
    worse(&mut branch, "late", "late");
    let verdicts = judged(&base, &branch, &[]);
    let line = verdicts
        .lines
        .iter()
        .find(|line| line.contains("moved beyond its usual spread (one seed"))
        .expect("a line");
    assert_eq!(
        line,
        "late/late share of orders arrived: 97.1% to 90.0% (-7.3%): moved beyond its usual spread (one seed; usually within 5%), worse"
    );
    assert!(
        has(
            &verdicts,
            "a move that was intended is explained by a line of the pull request description, `game-score: late/late arrived -7% because <reason>`; nothing here gates"
        ),
        "{:?}",
        verdicts.lines
    );
    assert!(!verdicts.lines.iter().any(|l| l.contains("NOT DECLARED")));
}

#[test]
fn a_move_inside_the_spread_stays_in_the_table_only() {
    let base = standard();
    let mut branch = standard();
    set(
        &mut branch,
        "late",
        "late",
        "service.economy.delivered_total",
        json!(43_000),
    );
    let verdicts = judged(&base, &branch, &[]);
    assert_eq!((verdicts.moved, verdicts.worse), (1, 0));
    assert!(quiet(&verdicts), "{:?}", verdicts.lines);
}

#[test]
fn a_small_count_that_moves_a_few_units_is_not_a_finding() {
    let mut base = standard();
    let mut branch = standard();
    set(
        &mut base,
        "late",
        "late",
        "service.planning.expired",
        json!(2),
    );
    set(
        &mut branch,
        "late",
        "late",
        "service.planning.expired",
        json!(7),
    );
    let verdicts = judged(&base, &branch, &[]);
    assert_eq!(verdicts.moved, 1);
    assert!(
        quiet(&verdicts),
        "2 to 7 is not a finding: {:?}",
        verdicts.lines
    );
    set(
        &mut branch,
        "late",
        "late",
        "service.planning.expired",
        json!(40),
    );
    assert!(has(
        &judged(&base, &branch, &[]),
        "route orders expired: 2 to 40"
    ));
}

#[test]
fn a_number_that_stays_inside_the_range_it_showed_across_seeds_is_not_a_finding() {
    let mut base = standard();
    let mut branch = standard();
    set(
        &mut base,
        "late",
        "late",
        "service.planning.stuck",
        json!(100),
    );
    set(
        &mut branch,
        "late",
        "late",
        "service.planning.stuck",
        json!(135),
    );
    let wide = with_entry(
        spread(1.0),
        ("late", "late"),
        "planning.stuck",
        Entry {
            median: 100.0,
            mad_percent: 1.0,
            min: 60.0,
            max: 140.0,
        },
    );
    assert!(
        quiet(&judged_with(&base, &branch, &[], &wide)),
        "inside 60 to 140"
    );
    assert!(has(
        &judged(&base, &branch, &[]),
        "route orders stuck: 100 to 135"
    ));
}

#[test]
fn a_larger_spread_widens_the_tolerance_to_twice_of_it() {
    let mut base = standard();
    let mut branch = standard();
    set(
        &mut base,
        "late",
        "late",
        "service.planning.stuck",
        json!(1000),
    );
    let noisy = |mad| {
        with_entry(
            spread(1.0),
            ("late", "late"),
            "planning.stuck",
            Entry {
                median: 1000.0,
                mad_percent: mad,
                min: 1000.0,
                max: 1000.0,
            },
        )
    };
    set(
        &mut branch,
        "late",
        "late",
        "service.planning.stuck",
        json!(1150),
    );
    assert!(
        quiet(&judged_with(&base, &branch, &[], &noisy(10.0))),
        "+15% is inside 2 x 10%"
    );
    assert!(
        has(
            &judged_with(&base, &branch, &[], &noisy(3.0)),
            "route orders stuck"
        ),
        "+15% is beyond 2 x 3%"
    );
    set(
        &mut branch,
        "late",
        "late",
        "service.planning.stuck",
        json!(1040),
    );
    assert!(
        quiet(&judged_with(&base, &branch, &[], &noisy(0.0))),
        "never under 5%: +4% is nothing"
    );
}

#[test]
fn a_number_nobody_measured_is_judged_at_twenty_five_percent() {
    let base = standard();
    let mut branch = standard();
    worse(&mut branch, "late", "late");
    assert!(
        quiet(&judged_with(&base, &branch, &[], &unmeasured())),
        "-7.3% is under 25%"
    );
    set(
        &mut branch,
        "late",
        "late",
        "service.arrival.arrived_permille",
        json!(700),
    );
    assert!(has(
        &judged_with(&base, &branch, &[], &unmeasured()),
        "usually within 25%"
    ));
}

#[test]
fn the_whole_window_is_shown_but_not_judged() {
    let base = standard();
    let mut branch = standard();
    worse(&mut branch, "natural", "whole");
    let verdicts = judged(&base, &branch, &[]);
    assert_eq!((verdicts.worse, verdicts.moved), (0, 1));
    assert!(quiet(&verdicts));
}

#[test]
fn a_judged_number_that_improved_beyond_its_spread_is_good_news_in_one_line() {
    let base = standard();
    let mut branch = standard();
    set(
        &mut branch,
        "late",
        "late",
        "service.arrival.order_to_arrival_ticks.p95",
        json!(40),
    );
    let verdicts = judged(&base, &branch, &[]);
    assert_eq!((verdicts.worse, verdicts.improved), (0, 1));
    assert!(
        has(
            &verdicts,
            "better, beyond its usual spread: late/late order to arrival, p95 (ticks): 61 to 40 (-34.4%)"
        ),
        "{:?}",
        verdicts.lines
    );
    assert!(
        verdicts
            .summary
            .contains("1 beyond their usual spread (0 worse, 0 explained)"),
        "{}",
        verdicts.summary
    );
}
