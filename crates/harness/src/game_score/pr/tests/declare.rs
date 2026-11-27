use super::*;

fn one(line: &str) -> Declaration {
    let (declared, unreadable) = parse(line);
    assert!(unreadable.is_empty(), "{unreadable:?}");
    assert_eq!(declared.len(), 1, "{line}");
    declared.into_iter().next().expect("one")
}

#[test]
fn a_declaration_names_a_scenario_a_number_a_change_and_a_reason() {
    let declaration = one("game-score: late arrived -3% because the AI waits for its escort");
    assert_eq!(
        declaration,
        Declaration {
            scenario: "late".to_owned(),
            window: None,
            number: "arrived".to_owned(),
            percent: -3.0,
            reason: "the AI waits for its escort".to_owned(),
        }
    );
}

#[test]
fn a_window_narrows_it() {
    let declaration = one("game-score: natural/early first-step-p95 +20.5% because queued orders");
    assert_eq!(declaration.window.as_deref(), Some("early"));
    assert_eq!(declaration.percent, 20.5);
    assert_eq!(declaration.number, "first-step-p95");
}

#[test]
fn a_bullet_backticks_capitals_and_a_typographic_minus_are_fine() {
    for line in [
        "- game-score: late arrived -3% because r",
        "* `game-score: late arrived -3% because r`",
        "  GAME-SCORE: late arrived \u{2212}3% because r  ",
        "> game-score: Late ARRIVED -3% because r",
    ] {
        let declaration = one(line);
        assert_eq!(declaration.percent, -3.0, "{line}");
        assert_eq!(declaration.number, "arrived", "{line}");
    }
}

#[test]
fn only_a_line_that_starts_with_the_prefix_is_read() {
    let (declared, unreadable) = parse(
        "The line game-score: late arrived -3% because r is quoted, not declared.\n\
         ## Why\n\ngame-score: late arrived -3% because the reason\nand more text",
    );
    assert_eq!(declared.len(), 1);
    assert!(unreadable.is_empty());
}

#[test]
fn a_line_that_starts_like_one_but_cannot_be_read_is_given_back() {
    for line in [
        "game-score: late arrived -3%",
        "game-score: late arrived -3% because",
        "game-score: late arrived -3% because   ",
        "game-score: late arrived worse because r",
        "game-score: late -3% because r",
        "game-score: late arrived -3% extra because r",
        "game-score: /early arrived -3% because r",
        "game-score: late/ arrived -3% because r",
    ] {
        let (declared, unreadable) = parse(line);
        assert!(declared.is_empty(), "{line}");
        assert_eq!(unreadable.len(), 1, "{line}");
    }
}

#[test]
fn the_line_of_a_declaration_reads_back_the_same() {
    let declaration =
        one("game-score: natural/early cost-per-arrival +12% because the planner is richer");
    assert_eq!(one(&declaration.line()), declaration);
}
