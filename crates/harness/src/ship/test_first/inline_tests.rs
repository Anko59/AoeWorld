use super::{
    Base, Kind, build_keys, inline, report,
    tests::{commit, repo},
};
use crate::ship::{git, tests::run};

const LIB: &str = "crates/sim/src/lib.rs";
const PRODUCT: &str = "pub fn f() {}\n";
const MODULE: &str = "pub fn f() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn a() {\n        super::f();\n    }\n}\n";
const TWO: &str = "pub fn f() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn a() {\n        super::f();\n    }\n\n    #[test]\n    fn b() {}\n}\n";

fn kinds(report: &super::Report) -> Vec<Kind> {
    report.commits.iter().map(|(_, kind)| *kind).collect()
}

#[test]
fn an_added_inline_test_module_is_a_test_commit() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(root, &[(LIB, MODULE)], "inline tests");
    let changed = MODULE.replace("pub fn f() {}", "pub fn f() { 1; }");
    commit(root, &[(LIB, &changed)], "impl");
    let report = report(root, &base, "HEAD").unwrap();
    assert_eq!(kinds(&report), [Kind::TestsOnly, Kind::Product]);
    assert_eq!(report.tests_before, 1);
    assert_eq!(report.inline, [LIB]);
    let text = report.markdown();
    assert!(
        text.contains("before the first product commit: 1\n"),
        "{text}"
    );
    assert!(
        text.contains("inline `#[cfg(test)]` changes in product files (the rule is tests in test files): crates/sim/src/lib.rs\n"),
        "{text}"
    );
}

#[test]
fn a_test_added_inside_an_existing_module_is_a_test_commit() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(root, &[(LIB, MODULE)], "inline tests");
    commit(root, &[(LIB, PRODUCT)], "tests removed");
    let after = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    commit(root, &[(LIB, MODULE)], "module back");
    commit(root, &[(LIB, TWO)], "one more test");
    let changed = TWO.replace("pub fn f() {}", "pub fn f() { 1; }");
    commit(root, &[(LIB, &changed)], "impl");
    let report = report(root, &after, "HEAD").unwrap();
    assert_eq!(
        kinds(&report),
        [Kind::TestsOnly, Kind::TestsOnly, Kind::Product]
    );
    assert_eq!(report.tests_before, 2);
    // Removing inline tests is a test change too.
    let removed = super::report(root, &base, &after).unwrap();
    assert_eq!(kinds(&removed), [Kind::TestsOnly, Kind::TestsOnly]);
}

#[test]
fn product_and_inline_test_lines_together_are_product() {
    let (temp, base) = repo();
    let root = temp.path();
    let changed = MODULE.replace("pub fn f() {}", "pub fn f() { 1; }");
    commit(root, &[(LIB, &changed)], "both");
    let report = report(root, &base, "HEAD").unwrap();
    assert_eq!(kinds(&report), [Kind::Product]);
    assert_eq!(report.inline, [LIB]);
    assert_eq!(report.tests_before, 0);
}

#[test]
fn test_regions_cover_cfg_test_items() {
    let text = "fn a() {}\n#[cfg(test)]\nmod b;\n#[cfg(test)] mod c { fn d() {} }\n#[cfg(all(test, unix))]\nfn e() {\n    // }\n    let x = 1;\n}\nfn f() {}\n";
    assert_eq!(inline::test_regions(text), [(2, 3), (4, 4), (5, 9)]);
    let patch = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -2,0 +3,2 @@\n+a\n+\n@@ -5 +7 @@\n-b\n+c\n";
    let changes = inline::changes(patch);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].0, "x.rs");
    assert_eq!(changes[0].1.new, [3, 7]);
    assert_eq!(changes[0].1.old, [5]);
}

#[test]
fn a_build_only_branch_is_reported() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(
        root,
        &[
            (".cargo/config.toml", "[build]\n"),
            ("rust-toolchain.toml", "[toolchain]\n"),
        ],
        "toolchain",
    );
    let report = report(root, &base, "HEAD").unwrap();
    assert!(report.applicable());
    let text = report.markdown();
    assert!(!text.contains("not applicable"), "{text}");
    assert!(
        text.contains("build-time files changed: .cargo/config.toml, rust-toolchain.toml\n"),
        "{text}"
    );
    assert!(
        text.contains("before the first product commit: none (no product commit)\n"),
        "{text}"
    );
}

#[test]
fn a_target_selector_change_is_a_build_change() {
    let unix = "[package]\nname = \"a\"\n[target.'cfg(unix)'.build-dependencies]\ncc = \"1\"\n";
    let windows =
        "[package]\nname = \"a\"\n[target.'cfg(windows)'.build-dependencies]\ncc = \"1\"\n";
    assert_ne!(build_keys(unix), build_keys(windows));
    let normal = "[package]\nname = \"a\"\n[target.'cfg(unix)'.dependencies]\ncc = \"1\"\n";
    let plain = "[package]\nname = \"a\"\n";
    assert_eq!(build_keys(normal), build_keys(plain));
}

#[test]
fn workspace_dependencies_inherited_by_build_scripts_are_build_changes() {
    let (temp, _) = repo();
    let root = temp.path();
    let workspace = |tool: &str, other: &str| {
        format!(
            "[workspace]\nmembers = [\"crates/sim\"]\n[workspace.dependencies]\ntool = \"{tool}\"\nother = \"{other}\"\n"
        )
    };
    commit(
        root,
        &[
            ("Cargo.toml", &workspace("1", "1")),
            (
                "crates/sim/Cargo.toml",
                "[package]\nname = \"sim\"\n[dependencies]\nother.workspace = true\n[build-dependencies]\ntool.workspace = true\n",
            ),
        ],
        "workspace",
    );
    let base = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    commit(root, &[("Cargo.toml", &workspace("1", "2"))], "other bump");
    let normal = report(root, &base, "HEAD").unwrap();
    assert!(normal.build.is_empty(), "{:?}", normal.build);
    commit(root, &[("Cargo.toml", &workspace("2", "2"))], "tool bump");
    let build = report(root, &base, "HEAD").unwrap();
    assert_eq!(build.build, ["Cargo.toml"]);
}

#[test]
fn a_merge_is_judged_against_its_first_parent() {
    let (temp, base) = repo();
    let root = temp.path();
    run(root, &["checkout", "-q", "-b", "side", &base]);
    commit(root, &[("README.md", "side\n")], "side");
    run(root, &["checkout", "-q", "feature"]);
    commit(root, &[("docs/x.md", "doc\n")], "docs");
    run(root, &["merge", "-q", "--no-ff", "--no-commit", "side"]);
    commit(root, &[(LIB, "pub fn f() { 1; }\n")], "merge side");
    let report = report(root, &base, "HEAD").unwrap();
    let merge = report.commits.last().unwrap().1;
    assert_eq!(merge, Kind::Merge(Base::Product));
    assert!(report.applicable());
    assert!(report.markdown().contains(" merge (product)\n"));
}

#[test]
fn binary_and_mode_only_changes_are_product_commits() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(root, &[("crates/sim/data.bin", "\0\u{1}\0")], "binary");
    run(root, &["update-index", "--chmod=+x", LIB]);
    run(root, &["commit", "-q", "-m", "mode"]);
    let report = report(root, &base, "HEAD").unwrap();
    assert_eq!(kinds(&report), [Kind::Product, Kind::Product]);
}

#[test]
fn custom_build_script_paths_are_normalized() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(
        root,
        &[
            (
                "crates/sim/Cargo.toml",
                "[package]\nname = \"sim\"\nbuild = \"./src/../../gen/./make.rs\"\n",
            ),
            ("crates/gen/make.rs", "fn main() {}\n"),
        ],
        "custom build",
    );
    let report = report(root, &base, "HEAD").unwrap();
    assert!(
        report.build.iter().any(|p| p == "crates/gen/make.rs"),
        "{:?}",
        report.build
    );
}
