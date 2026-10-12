use super::{authored, focus, runner};
use std::fs;

#[test]
fn overridden_build_failure_is_red_but_unknown_failure_is_not() {
    let build = runner::override_report(
        "error: could not compile `aoe-x` (lib test) due to 1 previous error\n",
        false,
    );
    assert!(build.build_failed);
    let unknown = runner::override_report("make: *** [duel-check] Error 1\n", false);
    assert!(!unknown.build_failed);
    assert!(unknown.failed.is_empty());
}

#[test]
fn ambiguous_unqualified_failure_cannot_be_attributed_to_a_changed_module() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("crates/x/src")).unwrap();
    fs::write(
        temp.path().join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\n",
    )
    .unwrap();
    for path in ["tests.rs", "other_tests.rs"] {
        fs::write(
            temp.path().join("crates/x/src").join(path),
            "#[test]\nfn same() {}\n",
        )
        .unwrap();
    }
    let report = focus::CheckReport {
        failed: vec!["same".into()],
        ..Default::default()
    };
    assert!(
        !authored::has_failure(temp.path(), &["crates/x/src/tests.rs".into()], &report).unwrap()
    );
    fs::remove_file(temp.path().join("crates/x/src/other_tests.rs")).unwrap();
    fs::create_dir_all(temp.path().join("crates/y/src")).unwrap();
    fs::write(
        temp.path().join("crates/y/src/tests.rs"),
        "#[test]\nfn same() {}\n",
    )
    .unwrap();
    assert!(
        !authored::has_failure(temp.path(), &["crates/x/src/tests.rs".into()], &report).unwrap()
    );
}

#[test]
fn unit_provenance_matches_real_binary_ids_and_human_override_names() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("crates/x/src")).unwrap();
    fs::write(
        temp.path().join("crates/x/Cargo.toml"),
        "[package]\nname='aoe-x'\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("crates/x/src/tests.rs"),
        "#[test]\nfn own() {}\n",
    )
    .unwrap();
    for (name, expected) in [
        ("aoe-x::aoe_x$tests::own", true),
        ("aoe-x tests::own", true),
        ("aoe-x::unrelated$tests::own", false),
        ("aoe-y::aoe_y$tests::own", false),
        ("aoe-x::aoe_x$other_tests::own", false),
    ] {
        let report = focus::CheckReport {
            failed: vec![name.into()],
            ..Default::default()
        };
        assert_eq!(
            authored::has_failure(temp.path(), &["crates/x/src/tests.rs".into()], &report).unwrap(),
            expected,
            "{name}"
        );
    }
}
