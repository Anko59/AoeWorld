use super::{Request, early, publish, run, summary};
use crate::ship::{
    Options, github, lock,
    stack::Parent,
    tests::{fixture, run as git_run},
};
use std::{fs, path::Path};

const BODY: &str = "<!-- level: low -->\n## Why\n> \"it broke\" — the user\n## What\nFix it.\n";
const BAD: &str = "<!-- level: low -->\n## Why\nOne.\nTwo.\nThree.\nFour.\n## What\nFix it.\n";

fn request(body: &Path) -> Request<'_> {
    Request {
        body,
        title: None,
        title_required: false,
        video: None,
        base: "dev",
        fetch: false,
    }
}

/// A checkout whose change touches `gates/`: the fixture's floor is `high`.
fn gated() -> (tempfile::TempDir, std::path::PathBuf) {
    let (temp, root) = fixture("true");
    fs::write(root.join("gates/note.txt"), "1\n2\n3\n4\n5\n6\n").unwrap();
    git_run(&root, &["add", "-A"]);
    git_run(&root, &["commit", "-q", "-m", "gates"]);
    (temp, root)
}

fn write(root: &Path, text: &str) -> std::path::PathBuf {
    let path = root.join("body.md");
    fs::write(&path, text).unwrap();
    path
}

fn problems(root: &Path, request: &Request) -> String {
    run(root, request).expect_err("problems").to_string()
}

#[test]
fn the_three_problems_of_issue_340_are_reported_in_one_message() {
    let (_temp, root) = gated();
    let body = write(&root, BAD);
    let message = problems(&root, &request(&body));
    assert!(message.contains("3 problems"), "{message}");
    assert!(message.contains("\n  1. `## Why` needs one grounding element"));
    assert!(message.contains("\n  2. `## Why` has more than 2 lines"));
    assert!(message.contains("\n  3. `low` is below the floor `high`"));
    assert!(!message.contains("\n  4."), "{message}");
}

#[test]
fn a_valid_description_reports_ok_with_level_and_floor() {
    let (_temp, root) = fixture("true");
    let body = write(&root, BODY);
    let checked = run(&root, &request(&body)).expect("valid");
    assert_eq!(
        summary(&checked.decision),
        "description: OK (level low, floor low)"
    );
}

#[test]
fn a_level_below_the_floor_names_the_floor_and_its_reason() {
    let (_temp, root) = gated();
    let body = write(&root, BODY);
    let message = problems(&root, &request(&body));
    assert!(message.contains("1 problem;"), "{message}");
    assert!(message.contains("below the floor `high`"), "{message}");
    assert!(
        message.contains("gate files: 6 of 13 changed lines (need 5 for high)"),
        "{message}"
    );
}

fn with_probe(root: &Path, banner: &str) {
    fs::write(root.join("probe.txt"), banner).unwrap();
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    fs::write(
        root.join("Makefile"),
        format!("{makefile}video-probe:\n\t@cat probe.txt\n"),
    )
    .unwrap();
    fs::write(root.join("clip.webm"), "x").unwrap();
}

#[test]
fn a_video_is_checked_against_the_level() {
    let (_temp, root) = gated();
    let body = write(&root, &BODY.replace("low", "high"));
    let clip = root.join("clip.webm");
    let mut with_video = request(&body);
    with_video.video = Some(&clip);
    let silent = "Input #0, x:\n  Duration: 00:02:30.00, start\n  Stream #0:0: Video: vp8\n";
    with_probe(&root, silent);
    let message = problems(&root, &with_video);
    assert!(
        message.contains("the video is 150s; a high PR allows 120s"),
        "{message}"
    );
    assert!(
        message.contains("a high PR video needs a voice-over"),
        "{message}"
    );
    let voiced = "Input #0, x:\n  Duration: 00:00:30.00\n  Stream #0:0: Video: vp8\n  Stream #0:1: Audio: opus\n";
    fs::write(root.join("probe.txt"), voiced).unwrap();
    assert!(run(&root, &with_video).is_ok());
    let message = problems(&root, &request(&body));
    assert!(message.contains("needs a showcase video"), "{message}");
}

#[test]
fn a_missing_or_unreadable_video_and_a_video_at_a_level_without_one() {
    let (_temp, root) = gated();
    let high = write(&root, &BODY.replace("low", "high"));
    let gone = root.join("gone.webm");
    let mut missing = request(&high);
    missing.video = Some(&gone);
    let message = problems(&root, &missing);
    assert!(message.contains("1 problem;"), "{message}");
    assert!(
        message.contains("SHIP_VIDEO") && message.contains("gone.webm"),
        "{message}"
    );
    fs::write(root.join("clip.webm"), "not a video").unwrap();
    let clip = root.join("clip.webm");
    let mut unreadable = request(&high);
    unreadable.video = Some(&clip);
    assert!(problems(&root, &unreadable).contains("is not a readable video"));
    let (_temp, plain) = fixture("true");
    let low = write(&plain, BODY);
    fs::write(plain.join("clip.webm"), "x").unwrap();
    let clip = plain.join("clip.webm");
    let mut at_low = request(&low);
    at_low.video = Some(&clip);
    assert!(problems(&plain, &at_low).contains("low PRs have no video"));
}

#[test]
fn the_title_is_checked_when_required_or_given() {
    let (_temp, root) = fixture("true");
    let body = write(&root, BODY);
    let mut needs = request(&body);
    needs.title_required = true;
    assert!(problems(&root, &needs).contains("needs SHIP_TITLE"));
    needs.title = Some("  ");
    assert!(problems(&root, &needs).contains("SHIP_TITLE is empty"));
    needs.title = Some("Fix it");
    assert!(run(&root, &needs).is_ok());
}

#[test]
fn an_unreadable_body_is_one_problem() {
    let (_temp, root) = fixture("true");
    let gone = root.join("nothing.md");
    assert!(problems(&root, &request(&gone)).contains("cannot be read"));
}

#[test]
fn make_ship_with_a_bad_description_stops_before_the_lock_and_the_gates() {
    let (_temp, root) = fixture("touch gate-ran");
    let options = Options {
        body_file: Some(write(&root, BAD)),
        title: Some("Fix it".into()),
        no_fetch: true,
        ..Options::default()
    };
    let error = publish(&root, &options).unwrap_err().to_string();
    assert!(error.contains("description: 2 problems"), "{error}");
    assert!(!root.join("gate-ran").exists(), "a gate ran");
    assert!(!lock::path(&root).unwrap().exists(), "the lock was taken");
}

fn open_pr(base: &str) -> github::OpenPr {
    serde_json::from_str(&format!(
        r#"{{"url":"https://github.com/o/r/pull/9","baseRefName":"{base}"}}"#
    ))
    .unwrap()
}

fn parent_pr(branch: &str) -> Parent {
    Parent {
        number: 7,
        branch: branch.into(),
        url: "https://github.com/o/r/pull/7".into(),
    }
}

/// The situation of a branch shipped to `base`, given the pull requests `gh` would list.
fn stand(
    root: &Path,
    base: &str,
    parent: Option<Parent>,
    own: Option<github::OpenPr>,
) -> github::Situation {
    github::situation_with(root, base, || Ok(parent), || Ok(own)).expect("situation")
}

#[test]
fn a_title_is_required_only_for_a_new_published_pull_request() {
    let (_temp, root) = fixture("true");
    git_run(&root, &["branch", "parent"]);
    // First ship to dev: required; an open pull request keeps its title.
    assert!(stand(&root, "dev", None, None).title_required());
    assert!(!stand(&root, "dev", None, Some(open_pr("dev"))).title_required());
    // Review ahead of a local parent without a pull request: nothing is published.
    assert!(!stand(&root, "parent", None, None).title_required());
    // Stacked on a parent that has its pull request: a new child needs a title.
    let child = stand(&root, "parent", Some(parent_pr("parent")), None);
    assert!(child.title_required() && !child.review_ahead);
    let open = stand(
        &root,
        "parent",
        Some(parent_pr("parent")),
        Some(open_pr("parent")),
    );
    assert!(!open.title_required());
}

#[test]
fn a_stacked_child_is_checked_against_its_local_parent_not_the_parents_lines() {
    let (_temp, root) = fixture("true");
    git_run(&root, &["checkout", "-q", "-b", "parent"]);
    fs::write(root.join("gates/note.txt"), "1\n2\n3\n4\n5\n6\n").unwrap();
    git_run(&root, &["add", "-A"]);
    git_run(&root, &["commit", "-q", "-m", "parent: gates"]);
    git_run(&root, &["checkout", "-q", "-b", "child"]);
    fs::write(root.join("child.txt"), "one\n").unwrap();
    git_run(&root, &["add", "-A"]);
    git_run(&root, &["commit", "-q", "-m", "child"]);
    let options = Options {
        body_file: Some(write(&root, BODY)),
        base: "parent".into(),
        ..Options::default()
    };
    let body = options.body_file.clone().unwrap();
    // Review ahead: no title yet is no problem, and the floor is the child's own (low).
    let ahead = stand(&root, "parent", None, None);
    let checked = early(&root, &options, &body, Some(&ahead), true).expect("child only");
    assert_eq!(
        summary(&checked.decision),
        "description: OK (level low, floor low)"
    );
    // Stacked on a published parent: the same child diff, but the title is required.
    drop(ahead);
    let stacked = stand(&root, "parent", Some(parent_pr("parent")), None);
    let _scope = crate::review::base::scope("parent");
    let message = early(&root, &options, &body, Some(&stacked), false)
        .expect_err("problems")
        .to_string();
    assert!(message.contains("needs SHIP_TITLE"), "{message}");
}

#[test]
fn without_the_situation_a_local_parent_cannot_be_resolved() {
    let (_temp, root) = fixture("true");
    git_run(&root, &["checkout", "-q", "-b", "parent"]);
    git_run(&root, &["checkout", "-q", "-b", "child"]);
    let options = Options {
        base: "parent".into(),
        ..Options::default()
    };
    let body = write(&root, BODY);
    let message = early(&root, &options, &body, None, true)
        .expect_err("problems")
        .to_string();
    assert!(
        message.contains("cannot be checked against the floor"),
        "{message}"
    );
}
