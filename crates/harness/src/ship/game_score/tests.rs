//! The game score in ship: the child `make`, the result file, and what is written to the pull
//! request. A scripted `make` and a recording `gh`; no build, no game, no GitHub.
use super::*;
use crate::ship::tests::{fixture, offline, run};
use std::cell::RefCell;

const HEAD: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const BASE: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

fn outcome(ran: bool) -> Outcome {
    Outcome {
        schema: 1,
        ran,
        head: HEAD.to_owned(),
        base: Some(BASE.to_owned()),
        summary: "informational: 1 number(s) worse, 1 not declared; 3 changed".to_owned(),
        section: format!("{MARKER}\n## Game score (informational)\n{ADDED}\n"),
        terminal: "game score (informational, nothing is gated): the table\n".to_owned(),
    }
}

fn evidence() -> Evidence {
    serde_json::from_value(serde_json::json!({
        "version": 1, "cadence": "preflight", "head": HEAD, "tree": "t", "branch": "feature",
        "base": "origin/dev", "base_branch": "dev", "merge_base": BASE, "changed": [],
        "gates": [], "verdict": "PASS", "level": null, "recheck": null, "started": 0,
        "finished": 0
    }))
    .expect("evidence")
}

fn result_json(outcome: &Outcome) -> String {
    serde_json::to_string(outcome).expect("json")
}

#[test]
fn the_child_is_told_the_merge_base_the_result_file_and_the_description() {
    assert_eq!(
        arguments(BASE, true),
        format!(
            "--base {BASE} --out .cache/game-score/pr.json --description .cache/game-score/description.md"
        )
    );
    assert_eq!(
        arguments(BASE, false),
        format!("--base {BASE} --out .cache/game-score/pr.json")
    );
}

#[test]
fn a_busy_long_job_slot_is_told_apart_from_a_failure() {
    assert!(busy(
        "resource: x\nlong job busy: slot 0: a, retry later\nmake: *** [..] Error 75"
    ));
    assert!(busy(
        "every build slot is held; do non-build work and retry later (`make load-status`)"
    ));
    assert!(!busy("error[E0432]: unresolved import"));
    assert!(!busy("long job busy"));
}

#[test]
fn a_result_written_for_another_commit_says_nothing_about_this_one() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("pr.json");
    fs::write(&path, result_json(&outcome(true))).expect("result");
    assert!(read(&path, HEAD).is_some_and(|outcome| outcome.ran));
    assert!(read(&path, BASE).is_none());
    fs::write(&path, "not json").expect("broken");
    assert!(read(&path, HEAD).is_none());
    assert!(read(&directory.path().join("absent.json"), HEAD).is_none());
}

#[test]
fn the_status_line_holds_the_summary_and_says_where_it_was_measured_when_the_head_moved() {
    let ran = outcome(true);
    assert_eq!(description(&ran, HEAD), ran.summary);
    let moved = description(&ran, BASE);
    assert!(
        moved.ends_with("(measured at a1a1a1a1a1a1)") || moved.chars().count() == 140,
        "{moved}"
    );
    let mut long = outcome(true);
    long.summary = "x".repeat(300);
    assert_eq!(description(&long, HEAD).chars().count(), 140);
}

#[test]
fn only_a_comment_of_a_person_with_write_access_that_starts_with_the_marker_is_ours() {
    let filter = find_filter();
    for expected in ["OWNER", "MEMBER", "COLLABORATOR", MARKER, ".id"] {
        assert!(filter.contains(expected), "{expected}");
    }
}

/// A checkout whose origin is a GitHub repository, and a `gh` that records its calls.
fn pull_request_world() -> (tempfile::TempDir, PathBuf) {
    let (temp, root) = fixture("true");
    run(
        &root,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:project/checkout.git",
        ],
    );
    (temp, root)
}

type Calls = RefCell<Vec<Vec<String>>>;

/// Answers `gh api` from `answers`: `(needle, reply)` for the first needle the call contains.
fn recording<'a>(
    calls: &'a Calls,
    answers: &'a [(&'a str, std::result::Result<&'a str, &'a str>)],
) -> impl Fn(&Path, &[&str]) -> Result<String> + 'a {
    move |_, args| {
        calls
            .borrow_mut()
            .push(args.iter().map(|arg| (*arg).to_owned()).collect());
        let joined = args.join(" ");
        for (needle, reply) in answers {
            if joined.contains(needle) {
                return match reply {
                    Ok(text) => Ok((*text).to_owned()),
                    Err(why) => Err(String::from(*why).into()),
                };
            }
        }
        Ok(String::new())
    }
}

fn has_call(calls: &Calls, needles: &[&str]) -> bool {
    calls
        .borrow()
        .iter()
        .any(|call| needles.iter().all(|needle| call.join(" ").contains(needle)))
}

const URL: &str = "https://github.com/project/checkout/pull/7";
const COMMENT: &str = "https://github.com/project/checkout/pull/7#issuecomment-99";

#[test]
fn a_first_ship_writes_a_new_comment_and_a_success_status_linking_to_it() {
    let (_temp, root) = pull_request_world();
    let calls = Calls::default();
    let answers = [(
        "--method POST repos/project/checkout/issues/7/comments",
        Ok(COMMENT),
    )];
    let gh = recording(&calls, &answers);
    publish_with(&gh, &root, URL, HEAD, Some(&outcome(true)));
    assert!(has_call(
        &calls,
        &["--paginate", "issues/7/comments", MARKER]
    ));
    assert!(has_call(
        &calls,
        &[
            "--method POST",
            "issues/7/comments",
            "body=<!-- aoe-game-score -->"
        ]
    ));
    assert!(has_call(
        &calls,
        &[
            "--method POST",
            &format!("repos/project/checkout/statuses/{HEAD}"),
            "state=success",
            "context=harness/game-score",
            "description=informational: 1 number(s) worse",
            &format!("target_url={COMMENT}"),
        ]
    ));
    assert!(!has_call(&calls, &["PATCH"]));
}

#[test]
fn a_reship_edits_the_newest_comment_in_place() {
    let (_temp, root) = pull_request_world();
    let calls = Calls::default();
    let answers = [
        ("--paginate", Ok("11\n42\n")),
        ("--method PATCH", Ok(COMMENT)),
    ];
    let gh = recording(&calls, &answers);
    publish_with(&gh, &root, URL, HEAD, Some(&outcome(true)));
    assert!(has_call(
        &calls,
        &[
            "--method PATCH",
            "repos/project/checkout/issues/comments/42",
            "body=<!-- aoe-game-score -->"
        ]
    ));
    assert!(!has_call(&calls, &["--method POST", "issues/7/comments"]));
    assert!(has_call(
        &calls,
        &["statuses", &format!("target_url={COMMENT}")]
    ));
}

#[test]
fn an_edit_that_fails_falls_back_to_a_new_comment() {
    let (_temp, root) = pull_request_world();
    let calls = Calls::default();
    let answers = [
        ("--paginate", Ok("42")),
        ("--method PATCH", Err("comment was deleted")),
        (
            "--method POST repos/project/checkout/issues/7/comments",
            Ok(COMMENT),
        ),
    ];
    let gh = recording(&calls, &answers);
    publish_with(&gh, &root, URL, HEAD, Some(&outcome(true)));
    assert!(has_call(&calls, &["--method POST", "issues/7/comments"]));
    assert!(has_call(
        &calls,
        &["statuses", &format!("target_url={COMMENT}")]
    ));
}

#[test]
fn a_comment_that_cannot_be_written_still_posts_the_status_without_a_link() {
    let (_temp, root) = pull_request_world();
    let calls = Calls::default();
    let answers = [("issues/7/comments", Err("403"))];
    let gh = recording(&calls, &answers);
    publish_with(&gh, &root, URL, HEAD, Some(&outcome(true)));
    assert!(has_call(&calls, &["statuses", "state=success"]));
    assert!(!has_call(&calls, &["target_url"]));
}

#[test]
fn the_status_is_never_a_failure() {
    let (_temp, root) = pull_request_world();
    let calls = Calls::default();
    let gh = recording(&calls, &[]);
    let mut crashed = outcome(true);
    crashed.summary = "informational: the game score crashed on this branch".to_owned();
    publish_with(&gh, &root, URL, HEAD, Some(&crashed));
    for call in calls
        .borrow()
        .iter()
        .filter(|call| call.join(" ").contains("statuses"))
    {
        let text = call.join(" ");
        assert!(text.contains("state=success"), "{text}");
        assert!(
            !text.contains("failure") && !text.contains("state=error"),
            "{text}"
        );
    }
}

#[test]
fn the_status_goes_on_the_pushed_commit_and_says_where_the_numbers_come_from() {
    let (_temp, root) = pull_request_world();
    let calls = Calls::default();
    let gh = recording(&calls, &[]);
    publish_with(&gh, &root, URL, BASE, Some(&outcome(true)));
    assert!(has_call(
        &calls,
        &[&format!("statuses/{BASE}"), "(measured at a1a1a1a1a1a1)"]
    ));
}

#[test]
fn nothing_is_written_when_the_game_score_did_not_run() {
    let (_temp, root) = pull_request_world();
    let calls = Calls::default();
    let gh = recording(&calls, &[]);
    publish_with(&gh, &root, URL, HEAD, Some(&outcome(false)));
    publish_with(&gh, &root, URL, HEAD, None);
    assert!(calls.borrow().is_empty());
}

#[cfg(unix)]
mod child;
