use crate::native::part::PreflightPart;
use clap::ValueEnum;

#[test]
fn all_runs_both_halves_and_each_half_runs_only_its_own() {
    assert!(PreflightPart::All.runs_checks() && PreflightPart::All.runs_unit());
    assert!(PreflightPart::Checks.runs_checks() && !PreflightPart::Checks.runs_unit());
    assert!(!PreflightPart::Unit.runs_checks() && PreflightPart::Unit.runs_unit());
}

#[test]
fn the_two_halves_together_run_everything_all_runs() {
    for (checks, unit) in [(true, false), (false, true)] {
        let part = PreflightPart::value_variants()
            .iter()
            .find(|part| (part.runs_checks(), part.runs_unit()) == (checks, unit))
            .expect("a part for each half");
        assert_ne!(*part, PreflightPart::All);
    }
}

#[test]
fn part_names_on_the_command_line_are_all_checks_and_unit() {
    let names: Vec<String> = PreflightPart::value_variants()
        .iter()
        .map(|part| part.to_possible_value().unwrap().get_name().to_owned())
        .collect();
    assert_eq!(names, ["all", "checks", "unit"]);
}
