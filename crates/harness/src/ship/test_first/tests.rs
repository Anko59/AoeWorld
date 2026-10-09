use super::{Kind, build_keys, exemption, report, summary, test_marker};
use crate::ship::{
    evidence::Verdict,
    git, ship,
    tests::{fixture, offline, run},
};
use std::{fs, path::Path};

/// A repository on `feature` from a `dev` base with a product crate.
fn repo() -> (tempfile::TempDir, String) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, &["init", "-q", "-b", "dev"]);
    write(
        root,
        "crates/sim/Cargo.toml",
        "[package]\nname = \"sim\"\nversion = \"0.1.0\"\n",
    );
    write(root, "crates/sim/src/lib.rs", "pub fn f() {}\n");
    write(root, "README.md", "x\n");
    run(root, &["add", "-A"]);
    run(root, &["commit", "-q", "-m", "base"]);
    let base = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    run(root, &["checkout", "-q", "-b", "feature"]);
    (temp, base)
}

fn write(root: &Path, path: &str, text: &str) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), text).unwrap();
}

fn commit(root: &Path, files: &[(&str, &str)], message: &str) {
    for (path, text) in files {
        write(root, path, text);
    }
    run(root, &["add", "-A"]);
    run(root, &["commit", "-q", "--allow-empty", "-m", message]);
}

const TESTS: &str = "#[test]\nfn a() {}\n#[tokio::test]\nasync fn b() {}\n#[cfg(test)]\nmod c;\n";

#[test]
fn commits_are_classified_and_tests_before_the_product_counted() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(root, &[("crates/sim/src/tests.rs", TESTS)], "tests");
    commit(root, &[("docs/x.md", "doc\n")], "docs");
    commit(
        root,
        &[("crates/sim/src/lib.rs", "pub fn f() { 1; }\n")],
        "impl",
    );
    commit(
        root,
        &[
            ("crates/sim/src/lib.rs", "pub fn f() { 2; }\n"),
            ("crates/sim/tests/more.rs", "#[test]\nfn d() {}\n"),
        ],
        "mixed",
    );
    let report = report(root, &base, "HEAD").unwrap();
    let kinds: Vec<Kind> = report.commits.iter().map(|(_, kind)| *kind).collect();
    assert_eq!(
        kinds,
        [Kind::TestsOnly, Kind::Other, Kind::Product, Kind::Mixed]
    );
    // `#[cfg(test)]` is no test function; the mixed commit came too late.
    assert_eq!(report.tests_before, 2);
    assert!(report.applicable() && report.build.is_empty() && report.exemption.is_none());
    let text = report.markdown();
    for line in [
        "tests-only, ",
        " other, ",
        " product, ",
        " mixed\n",
        "before the first product commit: 2\n",
        "build-time files changed: none\n",
        "test-first exemption: none\n",
    ] {
        assert!(text.contains(line), "{line}: {text}");
    }
}

#[test]
fn a_product_first_branch_reports_no_tests_before_it() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(
        root,
        &[("crates/sim/src/lib.rs", "pub fn f() { 1; }\n")],
        "impl",
    );
    commit(root, &[("crates/sim/src/tests.rs", TESTS)], "tests after");
    let report = report(root, &base, "HEAD").unwrap();
    assert_eq!(report.commits[0].1, Kind::Product);
    assert_eq!(report.tests_before, 0);
}

#[test]
fn build_time_files_are_detected() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(
        root,
        &[
            ("crates/sim/build.rs", "fn main() {}\n"),
            ("crates/sim/gen/make.rs", "fn main() {}\n"),
            (".cargo/config.toml", "[build]\n"),
            ("rust-toolchain.toml", "[toolchain]\n"),
            (".config/nextest.toml", "[profile.default]\n"),
            ("crates/core/.config/nextest.toml", "[profile.default]\n"),
            ("crates/other/Cargo.toml", "[package]\nname = \"other\"\n"),
        ],
        "build",
    );
    // A custom build script is read from the manifest, then changed.
    commit(
        root,
        &[(
            "crates/sim/Cargo.toml",
            "[package]\nname = \"sim\"\nversion = \"0.2.0\"\nbuild = \"./gen/make.rs\"\n",
        )],
        "custom build",
    );
    let report = report(root, &base, "HEAD").unwrap();
    for path in [
        "crates/sim/build.rs",
        "crates/sim/gen/make.rs",
        ".cargo/config.toml",
        "rust-toolchain.toml",
        ".config/nextest.toml",
        "crates/core/.config/nextest.toml",
        "crates/sim/Cargo.toml",
    ] {
        assert!(
            report.build.iter().any(|p| p == path),
            "{path}: {:?}",
            report.build
        );
    }
    // A new manifest without build keys is not a build-time change.
    assert!(!report.build.iter().any(|p| p == "crates/other/Cargo.toml"));
    assert!(report.markdown().contains("crates/sim/build.rs, "));
}

#[test]
fn only_build_keys_of_a_manifest_count() {
    let plain = "[package]\nname = \"a\"\nversion = \"1\"\n[dependencies]\nx = \"1\"\n";
    let bumped = "[package]\nname = \"a\"\nversion = \"2\"\n[dependencies]\nx = \"2\"\n";
    assert_eq!(build_keys(plain), build_keys(bumped));
    for changed in [
        "[package]\nname = \"a\"\nbuild = \"b.rs\"\n",
        "[package]\nname = \"a\"\nlinks = \"z\"\n",
        "[package]\nname = \"a\"\n[build-dependencies]\ncc = \"1\"\n",
        "[package]\nname = \"a\"\n[profile.test]\nopt-level = 3\n",
        "[package]\nname = \"a\"\n[target.'cfg(unix)'.build-dependencies]\ncc = \"1\"\n",
        "not = [toml",
    ] {
        assert_ne!(build_keys(plain), build_keys(changed), "{changed}");
    }
}

#[test]
fn harness_docs_and_ci_changes_are_not_applicable() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(
        root,
        &[
            ("crates/harness/src/x.rs", "fn x() {}\n"),
            ("gates/registry.json", "{}\n"),
            (".github/workflows/ci.yml", "on: push\n"),
            ("docs/x.md", "doc\n"),
        ],
        "harness",
    );
    let report = report(root, &base, "HEAD").unwrap();
    assert!(!report.applicable());
    assert!(report.markdown().contains("test-first: not applicable"));
}

#[test]
fn the_exemption_trailer_is_reported() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(
        root,
        &[("crates/sim/src/lib.rs", "pub fn g() {}\n")],
        "rename\n\nHarness-Test-First: exempt — pure rename",
    );
    let report = report(root, &base, "HEAD").unwrap();
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(report.exemption, Some((head, "pure rename".to_owned())));
    assert!(report.markdown().contains(": pure rename\n"));
    assert_eq!(exemption("exempt: docs"), Some("docs".to_owned()));
    assert_eq!(exemption("exempt - x"), Some("x".to_owned()));
    for refused in ["exempt", "exempt —  ", "exemptx", "required"] {
        assert_eq!(exemption(refused), None, "{refused}");
    }
}

#[test]
fn test_markers_are_attributes_naming_a_test() {
    for marker in [
        "#[test]",
        "  #[tokio::test(flavor = \"x\")]",
        "#[rstest]",
        "#[wasm_bindgen_test]",
    ] {
        assert!(test_marker(marker), "{marker}");
    }
    for other in [
        "#[cfg(test)]",
        "#[cfg_attr(test, x)]",
        "#[derive(Debug)]",
        "fn test() {}",
    ] {
        assert!(!test_marker(other), "{other}");
    }
}

#[test]
fn an_unreadable_history_is_reported_not_fatal() {
    let (temp, _) = repo();
    let text = summary(temp.path(), "no-such-revision", "HEAD");
    assert!(
        text.starts_with("- test-first report unavailable: "),
        "{text}"
    );
}

#[test]
fn the_report_reaches_the_review_facts() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(
        root,
        &[("crates/sim/src/lib.rs", "pub fn f() { 1; }\n")],
        "impl",
    );
    let facts = crate::review::facts(root, &base).unwrap();
    assert!(
        facts.contains("- test-first commits, oldest first: `"),
        "{facts}"
    );
    assert!(
        facts.contains("before the first product commit: 0\n"),
        "{facts}"
    );
}

#[test]
fn ship_proceeds_whatever_the_report_says() {
    let (_temp, root) = fixture("true");
    // Product code with no test before it, a build script and no exemption.
    write(&root, "crates/sim/src/lib.rs", "pub fn f() {}\n");
    write(&root, "crates/sim/build.rs", "fn main() {}\n");
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "untested product"]);
    let evidence = ship(&root, &offline()).expect("ship");
    assert_eq!(evidence.verdict, Verdict::Pass);
    let text = summary(&root, &evidence.merge_base, &evidence.head);
    assert!(
        text.contains("before the first product commit: 0\n"),
        "{text}"
    );
    assert!(text.contains("crates/sim/build.rs"), "{text}");
}
