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

#[test]
fn only_index_paths_classified_as_documentation_are_candidates() {
    let registry = crate::gates::registry::Registry::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    )
    .unwrap();
    let docs = paths(&["docs/testing.md", "README.md"]);
    assert!(super::super::checks::docs_only(
        &Kind::Index,
        &docs,
        &registry
    ));
    for files in [
        paths(&[]),
        paths(&["docs/testing.md", "crates/harness/src/main.rs"]),
        paths(&["Cargo.toml"]),
        paths(&["docs/data.json"]),
        paths(&["docs/embedded.rs"]),
        paths(&["../docs/escape.md"]),
        paths(&["docs/agent-engineering.md"]),
        paths(&["docs/agent-runtimes.md"]),
        paths(&["AGENTS.md"]),
        paths(&["unknown.md"]),
        paths(&["docs/testing.md", "deleted.rs"]),
        paths(&["docs/testing.md", "renamed.rs"]),
    ] {
        assert!(
            !super::super::checks::docs_only(&Kind::Index, &files, &registry),
            "{files:?}"
        );
    }
    for kind in [Kind::Working, Kind::Commit("0".repeat(40))] {
        assert!(!super::super::checks::docs_only(&kind, &docs, &registry));
    }
}

#[test]
fn documentation_selection_keeps_compilers_for_embedded_edits_and_deletions() {
    let repo = repo();
    let root = repo.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=[\"crates/example\"]\nresolver=\"3\"\n",
    );
    put(
        root,
        "crates/example/Cargo.toml",
        "[package]\nname=\"example\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    );
    put(
        root,
        "crates/example/src/lib.rs",
        "pub const GUIDE: &str = include_str!(\"../../../docs/embedded.md\");\n",
    );
    put(root, "docs/embedded.md", "original\n");
    put(
        root,
        "gates/registry.json",
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../gates/registry.json")).unwrap(),
    );
    crate::process::run_in(
        root,
        "cargo",
        &["generate-lockfile", "--offline"],
        &[],
        std::time::Duration::from_secs(30),
    )
    .unwrap();
    run(root, &["add", "."]);
    run(
        root,
        &["commit", "--quiet", "-m", "workspace with embedded guide"],
    );
    put(root, "docs/unread.md", "documentation only\n");
    run(root, &["add", "docs/unread.md"]);
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    assert!(super::super::checks::skip_compilers(&snapshot).unwrap());
    // Unstaged Rust edits cannot change the reader inventory of an Index scope.
    put(
        root,
        "crates/example/src/lib.rs",
        "pub const GUIDE: &str = \"unstaged\";\n",
    );
    put(root, "docs/embedded.md", "changed\n");
    run(root, &["add", "docs/embedded.md"]);
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    assert!(!super::super::checks::skip_compilers(&snapshot).unwrap());
    run(root, &["rm", "-f", "docs/embedded.md"]);
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    assert!(!super::super::checks::skip_compilers(&snapshot).unwrap());
}
