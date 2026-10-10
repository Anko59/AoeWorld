//! Stacked shipping (docs/shipping.md#stacked-pull-requests): the base is
//! `dev` or a branch with an open harness pull request, and everything is
//! measured against `refs/remotes/origin/<base>`.
use super::{
    super::{
        Options, git,
        github::{create_args, edit_args},
        judge,
        stack::{self, HeadPr, Parent},
    },
    fixture, offline, run,
};
use std::path::Path;

fn harness_pr(number: u64, state: &str) -> HeadPr {
    HeadPr {
        number,
        state: state.into(),
        url: format!("https://github.com/o/r/pull/{number}"),
        body: "# 🧑 For humans\n\n---\n\n# 🤖 For AI\n\n## Adversarial review\n".into(),
        cross_repository: false,
    }
}

#[test]
fn the_base_is_dev_or_an_unprotected_other_branch() {
    assert!(stack::refuse_base("dev", "feature").is_ok());
    assert!(stack::refuse_base("parent", "feature").is_ok());
    for protected in ["main", "release/1.0"] {
        let error = stack::refuse_base(protected, "feature").unwrap_err();
        assert!(error.contains("SHIP_BASE"), "{error}");
    }
    let error = stack::refuse_base("feature", "feature").unwrap_err();
    assert!(error.contains("itself"), "{error}");
    for bad in ["", "-x", "a..b", "a b"] {
        assert!(stack::refuse_base(bad, "feature").is_err(), "{bad:?}");
    }
}

#[test]
fn only_an_open_harness_pull_request_can_be_a_base() {
    let parent = stack::parent_from("parent", &[harness_pr(12, "OPEN")]).unwrap();
    assert_eq!(
        parent,
        Parent {
            number: 12,
            branch: "parent".into(),
            url: "https://github.com/o/r/pull/12".into(),
        }
    );
    // An earlier closed attempt does not hide the open one.
    let parent =
        stack::parent_from("parent", &[harness_pr(3, "CLOSED"), harness_pr(12, "OPEN")]).unwrap();
    assert_eq!(parent.number, 12);

    let merged = stack::parent_from("parent", &[harness_pr(12, "MERGED")]).unwrap_err();
    assert!(
        merged.contains("#12") && merged.contains("merged"),
        "{merged}"
    );
    assert!(merged.contains("rebase onto origin/dev"), "{merged}");
    assert!(merged.contains("SHIP_BASE=dev"), "{merged}");
    let closed = stack::parent_from("parent", &[harness_pr(12, "CLOSED")]).unwrap_err();
    assert!(closed.contains("closed") && closed.contains("rebase onto origin/dev"));
    let unknown = stack::parent_from("parent", &[]).unwrap_err();
    assert!(unknown.contains("no pull request"), "{unknown}");

    let mut foreign = harness_pr(12, "OPEN");
    foreign.cross_repository = true;
    assert!(stack::parent_from("parent", &[foreign]).is_err());
    let mut manual = harness_pr(12, "OPEN");
    manual.body = "opened by hand".into();
    let error = stack::parent_from("parent", &[manual]).unwrap_err();
    assert!(error.contains("make ship"), "{error}");
}

#[test]
fn gh_pull_request_listings_parse() {
    let prs: Vec<HeadPr> = serde_json::from_str(
        r#"[{"number":5,"state":"OPEN","url":"u","body":"b","isCrossRepository":false}]"#,
    )
    .unwrap();
    assert_eq!(prs[0].number, 5);
    assert!(!prs[0].cross_repository);
}

/// `feature` stacked on `parent`, both on origin: dev ← parent (PARENT.md) ← feature (README.md).
pub(in crate::ship) fn stacked_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let (temp, root) = fixture("true");
    run(&root, &["checkout", "-q", "-b", "parent", "dev"]);
    std::fs::write(root.join("PARENT.md"), "parent\n").unwrap();
    run(&root, &["add", "PARENT.md"]);
    run(&root, &["commit", "-q", "-m", "parent change"]);
    run(&root, &["push", "-q", "origin", "parent"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "parent"]);
    (temp, root)
}

fn on(base: &str) -> Options {
    Options {
        base: base.into(),
        no_fetch: false,
        ..offline()
    }
}

fn rev(root: &Path, name: &str) -> String {
    git::git(root, &["rev-parse", name]).unwrap()
}

#[test]
fn a_stacked_change_is_measured_against_the_fetched_remote_base() {
    let (_temp, root) = stacked_fixture();
    let parent = rev(&root, "parent");
    // Neither a stale remote-tracking ref nor a moved local branch counts.
    run(&root, &["update-ref", "-d", "refs/remotes/origin/parent"]);
    run(&root, &["branch", "-f", "parent", "dev"]);
    run(&root, &["branch", "origin/parent", "dev"]);
    let evidence = judge(&root, &on("parent")).unwrap();
    assert_eq!(evidence.base_branch, "parent");
    assert_eq!(evidence.base, parent);
    assert_eq!(evidence.merge_base, parent);
    assert_eq!(evidence.changed, ["README.md"]);
    // Against dev the parent's file is part of the change.
    let evidence = judge(&root, &on("dev")).unwrap();
    assert_eq!(evidence.base_branch, "dev");
    assert_eq!(evidence.changed, ["PARENT.md", "README.md"]);
}

#[test]
fn a_base_missing_on_origin_is_refused() {
    let (_temp, root) = fixture("true");
    let error = judge(&root, &on("nowhere")).unwrap_err().to_string();
    assert!(error.contains("nowhere"), "{error}");
}

#[test]
fn the_pull_request_targets_the_base_and_retargets_after_a_restack() {
    let create = create_args("o/r", "parent", "feature", "Title", "/b.md");
    let at = create.iter().position(|a| a == "--base").expect("--base");
    assert_eq!(create[at + 1], "parent");
    assert_eq!(&create[..2], ["pr", "create"]);
    assert!(create.windows(2).any(|w| w == ["--head", "feature"]));

    let edit = edit_args("u", "o/r", Some("T"), Some("/b.md"), Some("dev"));
    assert!(edit.windows(2).any(|w| w == ["--base", "dev"]), "{edit:?}");
    let edit = edit_args("u", "o/r", None, None, None);
    assert!(!edit.iter().any(|a| a == "--base"));

    assert_eq!(stack::retarget(Some("parent"), "dev"), Some("dev"));
    assert_eq!(stack::retarget(Some("dev"), "dev"), None);
    assert_eq!(stack::retarget(None, "parent"), None);
}

#[test]
fn the_description_and_next_steps_name_the_parent() {
    let parent = Parent {
        number: 12,
        branch: "parent".into(),
        url: "https://github.com/o/r/pull/12".into(),
    };
    let note = stack::note(&parent);
    assert!(note.contains("Stacked on #12") && note.contains("`parent`"));
    assert!(note.contains("reviewed against it"), "{note}");
    let next = stack::next_steps(Some(&parent), "https://github.com/o/r/pull/13");
    assert!(next.contains("not armed"), "{next}");
    assert!(next.contains("SHIP_BASE=dev"), "{next}");
    assert!(next.contains("rebase"), "{next}");
    let next = stack::next_steps(None, "u");
    assert!(next.contains("auto-merge"), "{next}");
}

#[test]
fn moving_a_pull_request_off_dev_disables_its_auto_merge() {
    use super::super::github::{auto_merge_not_enabled, disable_auto_args};
    assert_eq!(
        disable_auto_args("https://github.com/o/r/pull/13", "o/r"),
        [
            "pr",
            "merge",
            "https://github.com/o/r/pull/13",
            "--repo",
            "o/r",
            "--disable-auto"
        ]
    );
    assert!(auto_merge_not_enabled(
        "gh pr merge: GraphQL: Auto-merge is not enabled for this pull request"
    ));
    assert!(auto_merge_not_enabled("auto merge is not enabled"));
    assert!(!auto_merge_not_enabled("HTTP 502: bad gateway"));
}
