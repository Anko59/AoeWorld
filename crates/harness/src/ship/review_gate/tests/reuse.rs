use super::unstored;
use crate::{
    review::{
        Report, Tier,
        closing::CARRIED,
        protocol::{Finding, Reported, Severity, Status},
    },
    ship::{
        git, judge, review_gate,
        tests::{fixture, run},
    },
};
use std::{os::unix::ffi::OsStringExt, path::Path};

fn advance_dev(root: &Path) {
    run(root, &["checkout", "-q", "dev"]);
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(root, &["add", "BASE.md"]);
    run(root, &["commit", "-q", "-m", "move dev"]);
    run(root, &["push", "-q", "origin", "dev"]);
    run(root, &["fetch", "-q", "origin", "dev"]);
    run(root, &["checkout", "-q", "feature"]);
    run(root, &["rebase", "-q", "origin/dev"]);
}

fn source_report(root: &Path, tier: Tier, grade: u8) -> Report {
    let mut report = unstored(root, tier, grade);
    report.change_fingerprint = Some("stored-but-untrusted".into());
    report.store(root).unwrap();
    report
}

fn closing_source_report(root: &Path) -> Report {
    let mut report = unstored(root, Tier::Low, 6);
    report.closing = true;
    report.personas = vec!["correctness".into(), "spec".into(), "test-integrity".into()];
    report.findings.push(Finding {
        id: "F1".into(),
        reporter: CARRIED,
        round: 0,
        reported: Reported {
            file: "README.md".into(),
            line: None,
            severity: Severity::Major,
            category: "correctness".into(),
            claim: "carried finding fixed".into(),
            trigger: String::new(),
            expected_vs_actual: String::new(),
            evidence: String::new(),
        },
        votes: vec![],
        status: Status::Refuted,
    });
    assert!(report.passes());
    report.store(root).unwrap();
    report
}

fn advance_dev_with_grade(root: &Path, grade: u8) {
    run(root, &["checkout", "-q", "dev"]);
    let path = root.join("gates/review.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["merge_grade"] = grade.into();
    std::fs::write(path, config.to_string()).unwrap();
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(root, &["add", "gates/review.json", "BASE.md"]);
    run(root, &["commit", "-q", "-m", "raise review grade"]);
    run(root, &["push", "-q", "origin", "dev"]);
    run(root, &["fetch", "-q", "origin", "dev"]);
    run(root, &["checkout", "-q", "feature"]);
    run(root, &["rebase", "-q", "origin/dev"]);
}

fn alternate_change(root: &Path, reviewed: &[u8], candidate: &[u8]) -> String {
    std::fs::write(root.join("README.md"), reviewed).unwrap();
    run(root, &["add", "README.md"]);
    run(root, &["commit", "-q", "-m", "reviewed change"]);
    source_report(root, Tier::Low, 9);
    run(root, &["checkout", "-q", "-b", "candidate", "dev"]);
    std::fs::write(root.join("README.md"), candidate).unwrap();
    run(root, &["add", "README.md"]);
    run(root, &["commit", "-q", "-m", "candidate change"]);
    git::git(root, &["rev-parse", "HEAD"]).unwrap()
}

fn refuses_alternate_change(reviewed: &[u8], candidate: &[u8]) {
    let (_temp, root) = fixture("true");
    let head = alternate_change(&root, reviewed, candidate);
    assert!(
        Report::reuse_for_change(&root, &head, "candidate", &["low"])
            .unwrap()
            .is_none(),
        "distinct file blobs must receive a fresh review"
    );
}

fn eligible(tier: Tier) -> Vec<&'static str> {
    [Tier::Low, Tier::Medium, Tier::High, Tier::Xhigh, Tier::Max]
        .into_iter()
        .filter(|candidate| *candidate >= tier)
        .map(Tier::name)
        .collect()
}

#[test]
fn reuses_a_passing_review_after_rebase_onto_a_moved_base() {
    let (_temp, root) = fixture("true");
    let reviewed = source_report(&root, Tier::Low, 9);
    advance_dev(&root);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let reused = Report::reuse_for_change(&root, &head, "feature", &eligible(Tier::Low))
        .unwrap()
        .expect("same change should reuse");
    assert_eq!(reused.head, head);
    assert_eq!(reused.reused_from.as_deref(), Some(reviewed.head.as_str()));
    assert_eq!(
        reused.change_fingerprint.as_deref(),
        Some(
            git::change_fingerprint(&root, "dev", &head)
                .unwrap()
                .1
                .as_str()
        )
    );
    assert!(reused.passes());
    let stored = Report::load_passing(&root, &head, &["low"])
        .unwrap()
        .unwrap();
    assert_eq!(stored.reused_from.as_deref(), Some(reviewed.head.as_str()));
    let calls = review_gate::publish_calls("o/r", "u", &stored).unwrap();
    assert!(calls[0].join(" ").contains("description=review of "));
    assert!(
        calls[0]
            .join(" ")
            .contains("same change, identical file blobs")
    );

    let evidence = judge(&root, &crate::ship::tests::offline()).unwrap();
    let options = crate::ship::Options {
        no_review: false,
        ..crate::ship::tests::offline()
    };
    let reused = review_gate::require(&root, &evidence, &options, &|_, _, _, _, _| {
        panic!("same-change review should be reused")
    })
    .expect("review gate should accept the eligible reused report");
    assert_eq!(reused.reused_from.as_deref(), Some(reviewed.head.as_str()));
    let written = Report::load_passing(&root, &head, &["low"])
        .unwrap()
        .expect("reuse report should be written for the rebased head");
    assert_eq!(written.reused_from.as_deref(), Some(reviewed.head.as_str()));
    let status = review_gate::publish_calls("o/r", "u", &written).unwrap()[0].join(" ");
    assert!(
        status.contains(&format!(
            "description=review of {} reused (same change, identical file blobs)",
            &reviewed.head[..12]
        )),
        "{status}"
    );
}

#[test]
fn reuse_requires_the_current_merge_grade_and_records_it() {
    let (_temp, root) = fixture("true");
    let reviewed = source_report(&root, Tier::Low, 8);
    advance_dev_with_grade(&root, 9);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none(),
        "grade 8 must not be reused under today's grade 9 threshold"
    );
    assert_eq!(reviewed.grade, 8);

    let (_temp, root) = fixture("true");
    let reviewed = source_report(&root, Tier::Low, 8);
    advance_dev_with_grade(&root, 8);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let reused = Report::reuse_for_change(&root, &head, "feature", &["low"])
        .unwrap()
        .expect("grade 8 should be reusable under today's grade 8 threshold");
    assert_eq!(reused.reused_from.as_deref(), Some(reviewed.head.as_str()));
    assert_eq!(reused.merge_grade, 8);
    assert_eq!(reused.floor, "low");
}

#[test]
fn reuses_a_passing_closing_review_below_the_merge_grade() {
    let (_temp, root) = fixture("true");
    let reviewed = closing_source_report(&root);
    advance_dev(&root);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let reused = Report::reuse_for_change(&root, &head, "feature", &["low"])
        .unwrap()
        .expect("a passing closing review is reusable regardless of its grade");
    assert_eq!(reviewed.grade, 6);
    assert!(reused.closing);
    assert_eq!(reused.reused_from.as_deref(), Some(reviewed.head.as_str()));
    assert!(reused.passes());
}

#[test]
fn source_must_have_passed_its_stored_merge_grade() {
    let (_temp, root) = fixture("true");
    let mut reviewed = unstored(&root, Tier::Low, 8);
    reviewed.merge_grade = 9;
    // This represents an old failed report. Lowering today's threshold cannot
    // retroactively turn its stored verdict into a passing source.
    reviewed.store(&root).unwrap();
    assert!(!reviewed.passes());
    advance_dev_with_grade(&root, 8);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none(),
        "a historically failed report cannot become reusable under a lower current threshold"
    );
}

#[test]
fn raw_identity_keeps_arbitrary_path_bytes_without_text_decoding() {
    let (_temp, root) = fixture("true");
    let name = std::ffi::OsString::from_vec(b"raw-\x80-name".to_vec());
    std::fs::write(root.join(name), b"bytes\x80\n").unwrap();
    run(&root, &["add", "--all"]);
    run(&root, &["commit", "-q", "-m", "non-utf8 path"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let (merge_base, raw) = git::change_identity(&root, "dev", &head).unwrap();
    let expected = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args([
            "diff",
            "--raw",
            "--no-abbrev",
            "-z",
            "--no-renames",
            &format!("{merge_base}..{head}"),
        ])
        .output()
        .unwrap();
    assert!(expected.status.success());
    assert_eq!(
        raw, expected.stdout,
        "the comparison input must stay raw bytes"
    );
}

#[test]
fn a_one_line_patch_change_does_not_reuse() {
    let (_temp, root) = fixture("true");
    source_report(&root, Tier::Low, 9);
    advance_dev(&root);
    std::fs::write(root.join("README.md"), "changed differently\n").unwrap();
    run(&root, &["commit", "-q", "-am", "different patch"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none()
    );
}

#[test]
fn whitespace_only_line_difference_refuses_reuse() {
    refuses_alternate_change(b"#define F(x) x\n", b"#define G (x) x\n");
}

#[test]
fn string_literal_whitespace_difference_refuses_reuse() {
    refuses_alternate_change(
        b"const TOKEN: &str = \"a b\";\n",
        b"const TOKEN: &str = \"ab\";\n",
    );
}

#[test]
fn invalid_utf8_byte_difference_refuses_reuse() {
    refuses_alternate_change(b"text \x80\n", b"text \x81\n");
}

#[test]
fn a_rebase_that_changes_the_same_file_refuses_reuse() {
    let (_temp, root) = fixture("true");
    run(&root, &["checkout", "-q", "dev"]);
    std::fs::write(
        root.join("shared.txt"),
        "first\n\nthird\n\nfifth\n\nseventh\n\nninth\n\ntenth\n",
    )
    .unwrap();
    run(&root, &["add", "shared.txt"]);
    run(&root, &["commit", "-q", "-m", "add shared file"]);
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["fetch", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    std::fs::write(
        root.join("shared.txt"),
        "feature\n\nthird\n\nfifth\n\nseventh\n\nninth\n\ntenth\n",
    )
    .unwrap();
    run(&root, &["add", "shared.txt"]);
    run(&root, &["commit", "-q", "-m", "change first line"]);
    source_report(&root, Tier::Low, 9);
    run(&root, &["checkout", "-q", "dev"]);
    std::fs::write(
        root.join("shared.txt"),
        "first\n\nthird\n\nfifth\n\nseventh\n\nninth\n\ndev\n",
    )
    .unwrap();
    run(&root, &["add", "shared.txt"]);
    run(&root, &["commit", "-q", "-m", "change last line on dev"]);
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["fetch", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none()
    );
}

#[test]
fn complete_failing_report_for_head_blocks_reuse() {
    let (_temp, root) = fixture("true");
    let head = alternate_change(&root, b"reviewed\n", b"reviewed\n");
    let failure = unstored(&root, Tier::Low, 7);
    failure.store(&root).unwrap();
    assert!(failure.complete());
    assert!(!failure.passes());
    assert!(Report::has_complete(&root, &head, &["low"]).unwrap());
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none()
    );
}

#[test]
fn ship_reviews_head_again_when_it_has_a_complete_failure() {
    use std::cell::Cell;

    let (_temp, root) = fixture("true");
    let head = alternate_change(&root, b"reviewed\n", b"reviewed\n");
    let failure = unstored(&root, Tier::Low, 7);
    failure.store(&root).unwrap();
    let evidence = judge(&root, &crate::ship::tests::offline()).unwrap();
    assert_eq!(evidence.head, head);
    let options = crate::ship::Options {
        no_review: false,
        ..crate::ship::tests::offline()
    };
    let calls = Cell::new(0);
    review_gate::require(&root, &evidence, &options, &|root, tier, _, _, _| {
        calls.set(calls.get() + 1);
        Ok(unstored(root, tier, 9))
    })
    .expect("fresh review");
    assert_eq!(calls.get(), 1, "the passing source report was not reused");
}

#[test]
fn failing_and_reused_reports_are_never_reuse_sources() {
    let (_temp, root) = fixture("true");
    source_report(&root, Tier::Low, 7);
    advance_dev(&root);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none()
    );

    // A fresh repo gives us a passing original. Remove that original after
    // creating one reuse report, then prove the reuse report cannot be a hop.
    let (_second_temp, second) = fixture("true");
    let original = source_report(&second, Tier::Low, 9);
    advance_dev(&second);
    let rebased = git::git(&second, &["rev-parse", "HEAD"]).unwrap();
    Report::reuse_for_change(&second, &rebased, "feature", &["low"])
        .unwrap()
        .expect("first reuse");
    let common = git::git(
        &second,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .unwrap();
    let directory = Path::new(&common).join("aoe-ship/reviews");
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&format!("{}-", original.head))
        {
            std::fs::remove_file(path).unwrap();
        }
    }
    run(&second, &["checkout", "-q", "dev"]);
    std::fs::write(second.join("BASE2.md"), "another base movement\n").unwrap();
    run(&second, &["add", "BASE2.md"]);
    run(&second, &["commit", "-q", "-m", "move dev again"]);
    run(&second, &["push", "-q", "origin", "dev"]);
    run(&second, &["fetch", "-q", "origin", "dev"]);
    run(&second, &["checkout", "-q", "feature"]);
    run(&second, &["rebase", "-q", "origin/dev"]);
    let next = git::git(&second, &["rev-parse", "HEAD"]).unwrap();
    // Check the fresh repo's store, where only the reused report remains.
    assert!(
        Report::reuse_for_change(&second, &next, "feature", &["low"])
            .unwrap()
            .is_none()
    );
    assert!(directory.exists());
}

#[test]
fn reuse_respects_the_required_tier_floor() {
    let (_temp, root) = fixture("true");
    source_report(&root, Tier::Low, 9);
    advance_dev(&root);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["high"])
            .unwrap()
            .is_none()
    );
}
