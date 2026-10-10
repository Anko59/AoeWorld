//! A commit that stages only test files may hold red tests that do not
//! compile yet; everything else keeps the full clippy target set.
use super::super::checks::clippy_args;
use super::*;

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn a_tests_only_index_skips_test_targets_so_red_tests_can_be_committed() {
    let args = clippy_args(
        &Kind::Index,
        &paths(&[
            "crates/map/src/terrain/tests.rs",
            "crates/map/src/terrain/tests/landscape.rs",
            "browser/tests/landscape.spec.ts",
        ]),
    );
    assert!(!args.contains(&"--all-targets"), "{args:?}");
    assert!(args.contains(&"--workspace") && args.contains(&"--locked"));
    assert_eq!(&args[args.len() - 2..], ["-D", "warnings"]);
}

#[test]
fn any_product_file_or_an_empty_index_keeps_all_targets() {
    for staged in [
        paths(&[
            "crates/map/src/terrain/tests.rs",
            "crates/map/src/terrain.rs",
        ]),
        paths(&["Cargo.toml"]),
        paths(&[]),
    ] {
        assert!(
            clippy_args(&Kind::Index, &staged).contains(&"--all-targets"),
            "{staged:?}"
        );
    }
}

#[test]
fn working_and_commit_scopes_always_check_every_target() {
    let tests_only = paths(&["crates/map/src/terrain/tests.rs"]);
    for kind in [Kind::Working, Kind::Commit("0".repeat(40))] {
        assert!(clippy_args(&kind, &tests_only).contains(&"--all-targets"));
    }
}
