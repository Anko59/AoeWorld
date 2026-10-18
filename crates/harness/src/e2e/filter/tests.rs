use super::{FILTERED_REPORT, check_report, expected_report, parse_specs, refuse_in_ci, resolve};

fn candidates() -> Vec<String> {
    [
        "tests/village/selection-density.play.spec.ts",
        "tests/village/selection.play.spec.ts",
        "tests/village/commands.play.spec.ts",
        "tests/play.spec.ts",
        "tests/shared-surfaces.spec.ts",
    ]
    .map(str::to_owned)
    .into()
}

#[test]
fn specs_separate_on_spaces_and_commas_and_drop_empty_entries() {
    assert_eq!(
        parse_specs("tests/a.spec.ts, tests/b.spec.ts  tests/c.spec.ts,,"),
        ["tests/a.spec.ts", "tests/b.spec.ts", "tests/c.spec.ts"]
    );
    assert!(parse_specs(" , ").is_empty());
}

#[test]
fn a_literal_spec_path_selects_exactly_that_spec() {
    let found = resolve(
        &candidates(),
        &["tests/village/selection-density.play.spec.ts".into()],
    )
    .expect("one spec");
    assert_eq!(found, ["tests/village/selection-density.play.spec.ts"]);
}

#[test]
fn a_glob_selects_every_spec_it_names_once() {
    let found = resolve(
        &candidates(),
        &[
            "tests/village/*.play.spec.ts".into(),
            "tests/village/selection*.spec.ts".into(),
        ],
    )
    .expect("village specs");
    assert_eq!(
        found,
        [
            "tests/village/selection-density.play.spec.ts",
            "tests/village/selection.play.spec.ts",
            "tests/village/commands.play.spec.ts",
        ]
    );
}

#[test]
fn a_double_star_crosses_directories_and_a_single_star_does_not() {
    let all = resolve(&candidates(), &["tests/**/*.spec.ts".into()]).expect("all");
    assert_eq!(all.len(), 5);
    let top = resolve(&candidates(), &["tests/*.spec.ts".into()]).expect("top level");
    assert_eq!(top, ["tests/play.spec.ts", "tests/shared-surfaces.spec.ts"]);
}

#[test]
fn a_filter_that_matches_no_spec_fails_and_names_the_filter() {
    let error = resolve(&candidates(), &["tests/village/nope.spec.ts".into()])
        .expect_err("no spec matches");
    assert!(error.contains("tests/village/nope.spec.ts"), "{error}");
    assert!(error.contains("AOE_E2E_SPECS"), "{error}");
}

#[test]
fn a_pattern_matching_nothing_fails_even_beside_a_matching_one() {
    let found = resolve(
        &candidates(),
        &["tests/play.spec.ts".into(), "tests/missing.spec.ts".into()],
    );
    assert!(
        found.is_err(),
        "a pattern matching nothing must fail the run"
    );
}

#[test]
fn ci_refuses_a_filtered_run_and_only_a_filtered_run() {
    let specs = vec!["tests/play.spec.ts".to_owned()];
    assert!(refuse_in_ci(&specs, Some("true"), None).is_err());
    assert!(refuse_in_ci(&specs, None, Some("1")).is_err());
    let message = refuse_in_ci(&specs, Some("true"), None).expect_err("refused");
    assert!(message.contains("AOE_E2E_SPECS"), "{message}");
    assert!(refuse_in_ci(&[], Some("true"), Some("true")).is_ok());
    assert!(refuse_in_ci(&specs, None, None).is_ok());
    assert!(refuse_in_ci(&specs, Some(""), Some("")).is_ok());
}

#[test]
fn a_filtered_run_expects_only_its_own_report() {
    let specs = vec!["tests/play.spec.ts".to_owned()];
    assert_eq!(expected_report(&specs), Some(FILTERED_REPORT));
}

#[test]
fn the_full_suite_keeps_its_own_release_checks() {
    assert_eq!(expected_report(&[]), None);
}

#[test]
fn a_report_must_exist_and_hold_something() {
    let root = tempfile::tempdir().expect("root");
    let report = root.path().join(FILTERED_REPORT);
    std::fs::create_dir_all(report.parent().expect("parent")).expect("directory");
    assert!(check_report(root.path(), FILTERED_REPORT).is_err());
    std::fs::write(&report, b"").expect("empty");
    assert!(check_report(root.path(), FILTERED_REPORT).is_err());
    std::fs::write(&report, b"{}").expect("report");
    assert!(check_report(root.path(), FILTERED_REPORT).is_ok());
}
