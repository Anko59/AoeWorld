use super::super::{Base, Kind, report};
use super::{commit, repo};
use crate::ship::{git, tests::run};
use std::{path::Path, process::Command};

const LIB: &str = "crates/sim/src/lib.rs";
const OWN: &str = "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn own() {}\n}\n";

fn sha(root: &Path, rev: &str) -> String {
    git::git(root, &["rev-parse", rev]).unwrap()
}

fn advance_dev(root: &Path) -> String {
    run(root, &["checkout", "-q", "dev"]);
    commit(
        root,
        &[
            (
                "crates/sim/src/dev.rs",
                "pub fn incoming() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn incoming_test() {}\n}\n",
            ),
            (
                "crates/sim/tests/dev.rs",
                "#[test]\nfn incoming_a() {}\n#[test]\nfn incoming_b() {}\n#[test]\nfn incoming_c() {}\n",
            ),
            (".cargo/config.toml", "[build]\n"),
        ],
        "dev implementation and tests\n\nHarness-Test-First: exempt — dev reason",
    );
    let dev = sha(root, "HEAD");
    run(root, &["update-ref", "refs/remotes/origin/dev", &dev]);
    run(root, &["checkout", "-q", "feature"]);
    dev
}

fn updater_merge(root: &Path) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "merge",
            "-q",
            "--no-ff",
            "--no-edit",
            "dev",
        ])
        .env("GIT_COMMITTER_NAME", "GitHub")
        .env("GIT_COMMITTER_EMAIL", "noreply@github.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        git::git(root, &["show", "-s", "--format=%cn", "HEAD"]).unwrap(),
        "GitHub"
    );
    assert_eq!(
        git::git(root, &["show", "-s", "--format=%P", "HEAD"])
            .unwrap()
            .split_whitespace()
            .count(),
        2
    );
    sha(root, "HEAD")
}

#[test]
fn a_github_merge_of_dev_cannot_turn_a_docs_pr_into_product_work() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(root, &[("README.md", "own docs\n")], "own docs");
    let own = sha(root, "HEAD");
    let dev = advance_dev(root);
    let merge = updater_merge(root);
    for cutoff in [&base, &dev] {
        let report = report(root, cutoff, "HEAD").unwrap();
        assert_eq!(report.commits, [(own.clone(), Kind::Other)]);
        assert_eq!(report.tests_before, 0);
        assert!(report.inline.is_empty());
        assert!(
            report.build.is_empty(),
            "incoming dev build files are not the PR's: {:?}",
            report.build
        );
        assert!(report.exemption.is_none());
        assert!(!report.applicable());
        assert!(report.markdown().contains("not applicable"));
        assert!(
            !report
                .commits
                .iter()
                .any(|(id, _)| id == &merge || id == &dev)
        );
    }
}

#[test]
fn only_own_tests_inline_changes_and_exemptions_precede_the_own_implementation() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(
        root,
        &[
            (LIB, OWN),
            ("crates/sim/tests/own.rs", "#[test]\nfn own_external() {}\n"),
        ],
        "own tests\n\nHarness-Test-First: exempt — own reason",
    );
    let tests = sha(root, "HEAD");
    let dev = advance_dev(root);
    updater_merge(root);
    commit(
        root,
        &[(
            LIB,
            &OWN.replace("pub fn f() {}", "pub fn f() { let _ = 1; }"),
        )],
        "own implementation",
    );
    let implementation = sha(root, "HEAD");
    for cutoff in [&base, &dev] {
        let report = report(root, cutoff, "HEAD").unwrap();
        assert_eq!(
            report.commits,
            [
                (tests.clone(), Kind::TestsOnly),
                (implementation.clone(), Kind::Product)
            ]
        );
        assert_eq!(
            report.tests_before, 2,
            "incoming dev tests must not inflate the count"
        );
        assert_eq!(report.inline, [LIB]);
        assert!(report.build.is_empty());
        assert_eq!(report.exemption, Some((tests.clone(), "own reason".into())));
    }
}

#[test]
fn own_side_branch_tests_are_counted_once_when_their_merge_is_kept() {
    let (temp, base) = repo();
    let root = temp.path();
    run(root, &["checkout", "-q", "-b", "side"]);
    commit(
        root,
        &[(
            "crates/sim/tests/side.rs",
            "#[test]\nfn a() {}\n#[test]\nfn b() {}\n",
        )],
        "own side tests",
    );
    run(root, &["checkout", "-q", "feature"]);
    commit(root, &[("README.md", "own docs\n")], "own docs");
    run(root, &["merge", "-q", "--no-ff", "--no-edit", "side"]);
    commit(
        root,
        &[(LIB, "pub fn f() { let _ = 1; }\n")],
        "own implementation",
    );
    let report = report(root, &base, "HEAD").unwrap();
    assert_eq!(report.tests_before, 2);
    assert!(
        report
            .commits
            .iter()
            .any(|(_, kind)| matches!(kind, Kind::Merge(Base::TestsOnly)))
    );
}

#[test]
fn a_stacked_report_excludes_both_the_selected_parent_and_incoming_dev() {
    let (temp, base) = repo();
    let root = temp.path();
    run(root, &["checkout", "-q", "-b", "parent", &base]);
    commit(
        root,
        &[("crates/sim/src/parent.rs", "pub fn parent() {}\n")],
        "parent implementation",
    );
    let parent = sha(root, "HEAD");
    run(root, &["checkout", "-q", "feature"]);
    run(root, &["rebase", "-q", "parent"]);
    commit(root, &[("README.md", "child docs\n")], "child docs");
    let own = sha(root, "HEAD");
    advance_dev(root);
    updater_merge(root);
    let _scope = crate::review::base::scope_at("parent", &parent);
    let report = report(root, &base, "HEAD").unwrap();
    assert_eq!(report.commits, [(own, Kind::Other)]);
    assert!(!report.applicable());
    assert!(report.build.is_empty());
}

fn pending_dev_merge(root: &Path) -> String {
    commit(root, &[("README.md", "own docs\n")], "own docs");
    let docs = sha(root, "HEAD");
    advance_dev(root);
    run(root, &["merge", "-q", "--no-commit", "origin/dev"]);
    docs
}

#[test]
fn a_dev_merge_adding_its_own_product_requires_tests() {
    let (temp, base) = repo();
    let root = temp.path();
    let docs = pending_dev_merge(root);
    commit(
        root,
        &[("crates/simulation/src/own.rs", "pub fn untested() {}\n")],
        "merge dev with own implementation",
    );
    let report = report(root, &base, "HEAD").unwrap();
    assert_eq!(
        report.commits,
        [
            (docs, Kind::Other),
            (sha(root, "HEAD"), Kind::Merge(Base::Product))
        ]
    );
    assert!(report.applicable());
    assert_eq!(report.tests_before, 0);
    assert!(report.build.is_empty());
    assert!(report.inline.is_empty());
    assert!(report.exemption.is_none());
    assert!(
        report
            .markdown()
            .contains("before the first product commit: 0\n")
    );
}

#[test]
fn a_dev_merge_adding_only_own_tests_counts_them_before_product() {
    let (temp, base) = repo();
    let root = temp.path();
    let docs = pending_dev_merge(root);
    commit(
        root,
        &[("crates/simulation/tests/own.rs", "#[test]\nfn own() {}\n")],
        "merge dev with own tests",
    );
    let merge = sha(root, "HEAD");
    let tests = report(root, &base, "HEAD").unwrap();
    assert_eq!(
        tests.commits,
        [(docs, Kind::Other), (merge, Kind::Merge(Base::TestsOnly))]
    );
    assert!(!tests.applicable());
    assert!(tests.build.is_empty());
    commit(root, &[(LIB, "pub fn f() { let _ = 1; }\n")], "own product");
    let report = report(root, &base, "HEAD").unwrap();
    assert!(report.applicable());
    assert_eq!(report.tests_before, 1, "incoming dev tests must not count");
}

#[test]
fn a_dev_merge_adding_only_own_build_files_is_applicable() {
    let (temp, base) = repo();
    let root = temp.path();
    let docs = pending_dev_merge(root);
    commit(
        root,
        &[(".cargo/own.toml", "[build]\n")],
        "merge dev with own build config",
    );
    let report = report(root, &base, "HEAD").unwrap();
    assert_eq!(
        report.commits,
        [
            (docs, Kind::Other),
            (sha(root, "HEAD"), Kind::Merge(Base::Other))
        ]
    );
    assert_eq!(report.build, [".cargo/own.toml"]);
    assert!(report.applicable());
}

#[test]
fn a_dev_merge_resolving_a_conflict_by_hand_is_not_excluded() {
    let (temp, base) = repo();
    let root = temp.path();
    commit(root, &[("README.md", "own docs\n")], "own docs");
    let docs = sha(root, "HEAD");
    run(root, &["checkout", "-q", "dev"]);
    commit(root, &[("README.md", "dev docs\n")], "dev docs");
    let dev = sha(root, "HEAD");
    run(root, &["update-ref", "refs/remotes/origin/dev", &dev]);
    run(root, &["checkout", "-q", "feature"]);
    let merge = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "merge",
            "--no-commit",
            "origin/dev",
        ])
        .output()
        .unwrap();
    assert!(!merge.status.success());
    assert!(String::from_utf8_lossy(&merge.stdout).contains("CONFLICT"));
    assert!(git::git(root, &["merge-tree", "--write-tree", &docs, &dev]).is_err());
    commit(
        root,
        &[("README.md", "hand resolution\n")],
        "resolve dev conflict",
    );
    let report = report(root, &base, "HEAD").unwrap();
    assert_eq!(
        report.commits,
        [
            (docs, Kind::Other),
            (sha(root, "HEAD"), Kind::Merge(Base::Other))
        ]
    );
}
