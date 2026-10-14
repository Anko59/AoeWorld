use super::unstored;
use crate::slots::SlotEnv;
use crate::{
    agents::Runtime,
    review::{self, Plan, Report, Tier},
    ship::{
        Options,
        evidence::Verdict,
        git, ship_with,
        tests::{fixture, offline, run},
    },
};
use std::{cell::Cell, fs, path::Path};

fn local_parent(gate: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let (temp, root) = fixture(gate);
    run(&root, &["checkout", "-q", "-b", "parent", "dev"]);
    fs::write(root.join("PARENT.md"), "unpublished parent\n").unwrap();
    run(&root, &["add", "PARENT.md"]);
    run(&root, &["commit", "-q", "-m", "parent change"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "parent"]);
    (temp, root)
}

fn options() -> Options {
    Options {
        base: "parent".into(),
        review_ahead: true,
        no_review: false,
        ..offline()
    }
}

fn published(root: &Path) -> bool {
    !git::git(root, &["ls-remote", "origin", "refs/heads/feature"])
        .unwrap()
        .is_empty()
}

#[test]
fn local_parent_is_gated_and_reviewed_without_publishing() {
    let (_temp, root) = local_parent("echo gate >> ../gate-calls");
    let parent = git::git(&root, &["rev-parse", "refs/heads/parent"]).unwrap();
    let calls = Cell::new(0);
    let evidence = ship_with(&root, &options(), &|root, tier, _, _, plan| {
        assert_eq!(
            fs::read_to_string(root.join("../gate-calls")).unwrap(),
            "gate\n"
        );
        assert_eq!(*plan, Plan::Full);
        calls.set(calls.get() + 1);
        let report = unstored(root, tier, 9);
        assert_eq!(report.base, parent);
        assert_eq!(report.base_branch, "parent");
        report.store(root).unwrap();
        Ok(report)
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(evidence.verdict, Verdict::Pass);
    assert_eq!(evidence.base, parent);
    assert_eq!(evidence.changed, ["README.md"]);
    assert!(!published(&root));
    assert_eq!(review::history(&root, "feature").unwrap().len(), 1);

    // A second ahead attempt still runs its gates, then uses the stored approval.
    ship_with(&root, &options(), &|_, _, _, _, _| {
        panic!("already approved")
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(root.join("../gate-calls")).unwrap(),
        "gate\ngate\n"
    );
    assert!(!published(&root));

    // Once the parent is on origin, the normal publication path uses this review.
    run(&root, &["push", "-q", "origin", "parent"]);
    let publish = Options {
        review_ahead: false,
        no_fetch: false,
        ..options()
    };
    ship_with(&root, &publish, &|_, _, _, _, _| {
        panic!("ahead approval survives")
    })
    .unwrap();
    assert!(published(&root));
}

#[test]
fn local_review_ahead_pins_the_parent_even_if_the_ref_moves_during_gates() {
    let (_temp, root) = local_parent("git branch -f parent dev");
    let parent = git::git(&root, &["rev-parse", "parent"]).unwrap();
    let evidence = ship_with(&root, &options(), &|root, tier, _, _, _| {
        assert_ne!(git::git(root, &["rev-parse", "parent"]).unwrap(), parent);
        let report = unstored(root, tier, 9);
        assert_eq!(report.base, parent);
        report.store(root).unwrap();
        Ok(report)
    })
    .unwrap();
    assert_eq!(evidence.base, parent);
    assert!(!published(&root));
}

#[test]
fn failed_ahead_gates_never_start_a_review_or_publish() {
    let (_temp, root) = local_parent("false");
    let error = ship_with(&root, &options(), &|_, _, _, _, _| panic!("gates first"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("FAIL"), "{error}");
    assert!(!published(&root));
    assert!(review::history(&root, "feature").unwrap().is_empty());
}

#[test]
fn a_failed_ahead_review_is_stored_but_never_published() {
    let (_temp, root) = local_parent("true");
    let error = ship_with(&root, &options(), &|root, tier, _, _, _| {
        let report = unstored(root, tier, 7);
        report.store(root).unwrap();
        Ok(report)
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("closing review"), "{error}");
    assert!(!published(&root));
    assert_eq!(review::history(&root, "feature").unwrap().len(), 1);
}

#[test]
fn standalone_review_accepts_the_same_local_base_and_runs_gates() {
    let (_temp, root) = local_parent("echo gate >> ../gate-calls");
    let parent = git::git(&root, &["rev-parse", "parent"]).unwrap();
    let report = review::review_on_base(
        &root,
        "parent",
        Tier::Low,
        Runtime::Claude,
        "task",
        &|root, tier, _, task, _| {
            assert!(root.join("../gate-calls").is_file());
            assert_eq!(task, "task");
            let mut report = unstored(root, tier, 9);
            report.task_fingerprint = Some(Report::task_fingerprint(task));
            report.store(root).unwrap();
            Ok(report)
        },
        &SlotEnv::default(),
    )
    .unwrap();
    assert_eq!(report.base, parent);
    assert!(!published(&root));
}

#[test]
fn ahead_status_names_the_grade_and_unpublished_parent() {
    let (_temp, root) = local_parent("true");
    let report = unstored(&root, Tier::Low, 9);
    assert_eq!(
        crate::ship::stack::ahead_message("parent", &report),
        "reviewed ahead: 9/10; push when the parent parent has its pull request"
    );
}
