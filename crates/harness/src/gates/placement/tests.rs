use super::*;

const PRODUCT: &str = "crates/simulation/src/lib.rs";
const INLINE: &str =
    "pub fn run() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn old() {}\n}\n";

fn run(root: &Path, args: &[&str]) -> String {
    String::from_utf8(git(root, args).unwrap())
        .unwrap()
        .trim()
        .to_owned()
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn commit(root: &Path) -> String {
    run(root, &["add", "-A"]);
    let output = command(root)
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .args(["commit", "-qm", "fixture"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    run(root, &["rev-parse", "HEAD"])
}

fn fixture(initial: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    run(repo.path(), &["init", "-q", "--template="]);
    write(repo.path(), PRODUCT, initial);
    let base = commit(repo.path());
    run(
        repo.path(),
        &["update-ref", "refs/remotes/origin/dev", &base],
    );
    repo
}

fn error(root: &Path) -> String {
    check(root, Kind::Working).unwrap_err().to_string()
}

#[test]
fn new_inline_module_fails_with_path_line_and_remedy_including_harness() {
    for path in [PRODUCT, "crates/harness/src/example.rs"] {
        let repo = fixture("pub fn run() {}\n");
        write(repo.path(), path, INLINE);
        let message = error(repo.path());
        assert!(message.contains(&format!("{path}:2:")), "{message}");
        assert!(message.contains("<module>/tests.rs (or <name>_tests.rs)"));
        assert!(message.contains("#[cfg(test)] mod tests;"));
    }
}

#[test]
fn test_style_functions_fail_even_without_cfg_or_inside_ordinary_modules() {
    for marker in ["test", "tokio::test", "rstest", "wasm_bindgen_test"] {
        let repo = fixture("pub fn run() {}\n");
        write(
            repo.path(),
            PRODUCT,
            &format!("pub fn run() {{}}\nmod inner {{\n#[{marker}]\nfn added() {{}}\n}}\n"),
        );
        assert!(error(repo.path()).contains(&format!("{PRODUCT}:3:")));
    }
    let repo = fixture("pub fn run() {}\n");
    write(
        repo.path(),
        PRODUCT,
        "struct S;\nimpl S { #[test] fn added() {} }\n",
    );
    assert!(error(repo.path()).contains(&format!("{PRODUCT}:2:")));
}

#[test]
fn existing_inline_tests_are_grandfathered_but_every_new_line_fails() {
    let repo = fixture(INLINE);
    check(repo.path(), Kind::Working).unwrap();
    for line in ["    fn added() {}\n", "    // new comment\n", "\n"] {
        write(
            repo.path(),
            PRODUCT,
            &INLINE.replace("    #[test]", &format!("{line}    #[test]")),
        );
        assert!(error(repo.path()).contains(&format!("{PRODUCT}:4:")));
    }
    write(
        repo.path(),
        PRODUCT,
        &INLINE.replace("fn old() {}", "fn old() { let added = 1; }"),
    );
    assert!(error(repo.path()).contains(&format!("{PRODUCT}:5:")));
    write(
        repo.path(),
        PRODUCT,
        &format!("// product comment\n{INLINE}"),
    );
    check(repo.path(), Kind::Working).unwrap();
}

#[test]
fn standalone_existing_test_functions_cannot_gain_lines() {
    let repo = fixture("#[test]\nfn old() {\n}\n");
    write(
        repo.path(),
        PRODUCT,
        "#[test]\nfn old() {\n    assert!(true);\n}\n",
    );
    assert!(error(repo.path()).contains(&format!("{PRODUCT}:3:")));
}

#[test]
fn moving_removing_and_declaring_external_tests_pass() {
    let repo = fixture(INLINE);
    write(
        repo.path(),
        PRODUCT,
        "pub fn run() {}\n#[cfg(test)]\nmod tests;\n",
    );
    write(
        repo.path(),
        "crates/simulation/src/tests.rs",
        "#[test]\nfn old() {}\n",
    );
    check(repo.path(), Kind::Working).unwrap();
    fs::remove_file(repo.path().join(PRODUCT)).unwrap();
    check(repo.path(), Kind::Working).unwrap();
    write(
        repo.path(),
        PRODUCT,
        "#[cfg(all(unix, test))] mod custom_tests;\n",
    );
    check(repo.path(), Kind::Working).unwrap();
}

#[test]
fn nested_cfgs_multiline_attributes_and_braces_in_strings_are_parsed() {
    for cfg in [
        "all(unix, test)",
        "any(test, unix)",
        "all(any(unix, test), not(windows))",
    ] {
        let repo = fixture("pub fn run() {}\n");
        write(
            repo.path(),
            PRODUCT,
            &format!("#[cfg(\n{cfg}\n)]\nmod t {{ fn helper() {{ let _ = \"}} // {{\"; }} }}\n"),
        );
        assert!(error(repo.path()).contains(&format!("{PRODUCT}:1:")));
    }
    let repo = fixture("pub fn run() {}\n");
    write(
        repo.path(),
        PRODUCT,
        "#[cfg(test)] fn helper() { mod inner { fn f() {} } }\n",
    );
    assert!(error(repo.path()).contains(&format!("{PRODUCT}:1:")));
}

#[test]
fn not_test_cfg_and_test_named_ordinary_functions_pass() {
    let repo = fixture("pub fn run() {}\n");
    write(
        repo.path(),
        PRODUCT,
        "#[cfg(not(test))]\nmod product { fn test_named() {} }\n#[cfg(all(unix, not(test)))] mod also_product {}\n#[cfg(feature = \"test\")] mod feature {}\n#[cfg(test)] fn helper() {}\nstruct S; impl S { fn method() {} }\n",
    );
    check(repo.path(), Kind::Working).unwrap();
}

#[test]
fn test_files_and_paths_outside_crates_are_unaffected_even_when_unparsable() {
    let repo = fixture("pub fn run() {}\n");
    for path in [
        "crates/simulation/src/tests.rs",
        "crates/simulation/src/tests/one.rs",
        "crates/simulation/src/unit_tests.rs",
        "crates/simulation/src/one_test.rs",
        "crates/simulation/testdata/fixture.rs",
        "tools/product.rs",
    ] {
        write(repo.path(), path, "#[test] fn broken(\n");
    }
    check(repo.path(), Kind::Working).unwrap();
}

#[test]
fn changed_unparsable_rust_is_reported_without_panicking() {
    let repo = fixture("pub fn run() {}\n");
    write(repo.path(), PRODUCT, "pub fn run() {\n");
    let message = error(repo.path());
    assert!(
        message.contains(&format!("{PRODUCT}:1: unparsable Rust file:")),
        "{message}"
    );
    let repo = fixture("pub fn incomplete(\n");
    check(repo.path(), Kind::Working).unwrap();
}

#[test]
fn missing_remote_and_shallow_missing_merge_base_fail_closed() {
    let repo = fixture("pub fn run() {}\n");
    run(
        repo.path(),
        &["update-ref", "-d", "refs/remotes/origin/dev"],
    );
    assert!(
        error(repo.path())
            .contains("comparison base unavailable: fetch origin/dev and full history")
    );
    run(repo.path(), &["branch", "base"]);
    write(repo.path(), PRODUCT, "pub fn new() {}\n");
    commit(repo.path());
    let clone = tempfile::tempdir().unwrap();
    let url = format!("file://{}", repo.path().display());
    run(clone.path(), &["clone", "-q", "--depth=1", &url, "."]);
    run(
        clone.path(),
        &[
            "fetch",
            "-q",
            "--depth=1",
            "origin",
            "base:refs/remotes/origin/dev",
        ],
    );
    assert!(error(clone.path()).contains("comparison base unavailable"));
}

#[test]
fn index_and_commit_scopes_read_exact_snapshot_and_keep_branch_additions() {
    let repo = fixture("pub fn run() {}\n");
    write(repo.path(), PRODUCT, INLINE);
    run(repo.path(), &["add", "-A"]);
    write(repo.path(), PRODUCT, "pub fn run() {}\n");
    check(repo.path(), Kind::Working).unwrap();
    assert!(
        check(repo.path(), Kind::Index)
            .unwrap_err()
            .to_string()
            .contains(&format!("{PRODUCT}:2:"))
    );
    write(repo.path(), PRODUCT, INLINE);
    let candidate = commit(repo.path());
    // A clean checkout still must compare the whole branch, not just HEAD's diff.
    assert!(error(repo.path()).contains(&format!("{PRODUCT}:2:")));
    write(repo.path(), PRODUCT, "pub fn run() {}\n");
    assert!(
        check(repo.path(), Kind::Commit(candidate))
            .unwrap_err()
            .to_string()
            .contains(&format!("{PRODUCT}:2:"))
    );
}

#[test]
fn comparison_uses_merge_base_even_when_origin_dev_advances_and_tag_shadows_it() {
    let repo = fixture("pub fn run() {}\n");
    run(repo.path(), &["branch", "feature"]);
    write(
        repo.path(),
        "crates/simulation/src/other.rs",
        "pub fn other() {}\n",
    );
    let remote = commit(repo.path());
    run(
        repo.path(),
        &["update-ref", "refs/remotes/origin/dev", &remote],
    );
    run(repo.path(), &["checkout", "-q", "feature"]);
    write(repo.path(), PRODUCT, INLINE);
    commit(repo.path());
    run(repo.path(), &["tag", "origin/dev"]);
    assert!(error(repo.path()).contains(&format!("{PRODUCT}:2:")));
}

#[test]
fn added_and_renamed_product_files_and_unusual_paths_are_checked() {
    let repo = fixture("pub fn run() {}\n");
    let path = "crates/simulation/src/snow ☃ space.rs";
    write(repo.path(), path, INLINE);
    assert!(error(repo.path()).contains(&format!("{path}:2:")));
    fs::remove_file(repo.path().join(path)).unwrap();
    write(repo.path(), "crates/simulation/src/tests.rs", INLINE);
    commit(repo.path());
    fs::rename(
        repo.path().join("crates/simulation/src/tests.rs"),
        repo.path().join(path),
    )
    .unwrap();
    assert!(error(repo.path()).contains(&format!("{path}:2:")));
}

#[test]
fn registry_matches_structure_cadences_blocks_and_static_job() {
    use crate::gates::registry::{Cadence, Registry};
    let registry = Registry::load(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .as_path(),
    )
    .unwrap();
    let structure = registry
        .gates
        .iter()
        .find(|g| g.id == "structure-check")
        .unwrap();
    let placement = registry
        .gates
        .iter()
        .find(|g| g.id == "test-placement")
        .unwrap();
    assert_eq!(placement.cadences, structure.cadences);
    assert_eq!(placement.blocks, structure.blocks);
    assert_eq!(placement.budget_s, structure.budget_s);
    assert!(placement.r#static);
    for cadence in [
        Cadence::Commit,
        Cadence::Stop,
        Cadence::Pr,
        Cadence::Ci,
        Cadence::Preflight,
    ] {
        let plan = registry
            .plan(
                cadence,
                &std::collections::BTreeSet::from(["static".to_owned()]),
            )
            .unwrap();
        assert!(plan.gates.iter().any(|g| g == "test-placement"));
    }
    assert!(
        registry.jobs["static"]
            .iter()
            .any(|g| g == "test-placement")
    );
}

#[test]
fn cfg_on_enclosing_items_and_file_cannot_hide_new_inline_modules() {
    for text in [
        "#![cfg(test)]\nmod t {}\n",
        "#[cfg(test)] impl S { fn helper() { mod t {} } }\n",
        "impl S { #[cfg(test)] fn helper() { mod t {} } }\n",
        "#[cfg(test)] trait T { fn helper() { mod t {} } }\n",
        "#[cfg(test)] const N: u32 = { mod t {} 0 };\n",
        "#[cfg(test)] static N: u32 = { mod t {} 0 };\n",
        "trait T { #[test] fn helper() {} }\n",
    ] {
        let repo = fixture("pub fn run() {}\n");
        write(repo.path(), PRODUCT, text);
        assert!(error(repo.path()).contains(&format!("{PRODUCT}:1:")));
    }
    let repo = fixture("fn helper() {\nmod t {}\n}\n");
    write(
        repo.path(),
        PRODUCT,
        "#[cfg(test)]\nfn helper() {\nmod t {}\n}\n",
    );
    assert!(error(repo.path()).contains(&format!("{PRODUCT}:1:")));
}
