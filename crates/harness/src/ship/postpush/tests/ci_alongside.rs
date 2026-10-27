//! Scripted GitHub, local bare origin, and the actual ship orchestration/CLI.
use super::stub::World;
use crate::ship::{Options, git, review_gate::graded, ship_with, tests::run};
use std::fs;

const BODY: &str = "<!-- level: low -->\n## Why\n> \"CI alongside review\" — the owner\n## What\nStart CI during review.\n";
const GH: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$GH_LOG"
case "$1 $2" in
  '--version ') echo 'gh version 2.100.0'; exit 0 ;;
  'pr list')
    case "$*" in *'--state merged'*) echo '[]'; exit 0 ;; esac
    if [ -f "$GH_DIR/existing" ]; then echo '{"url":"https://github.com/project/checkout/pull/1","baseRefName":"dev"}'; fi ;;
  'pr view')
    d=false; [ -f "$GH_DIR/draft" ] && d=true
    a=false; [ -f "$GH_DIR/auto" ] && a=true
    case "$*" in *--jq*) echo "the draft state is parsed in Rust, never with --jq" >&2; exit 9 ;; esac
    m=null; [ "$a" = true ] && m='{"enabledAt":"2026-10-21T00:00:00Z","mergeMethod":"SQUASH"}'
    printf '{"isDraft":%s,"autoMergeRequest":%s}\n' "$d" "$m" ;;
  'pr create')
    case "$*" in *--draft*) touch "$GH_DIR/draft" ;; esac
    touch "$GH_DIR/existing"
    echo 'https://github.com/project/checkout/pull/1' ;;
  'pr ready')
    case "$*" in
      *--undo*) [ ! -f "$GH_DIR/auto" ] || exit 12; touch "$GH_DIR/draft" ;;
      *) rm -f "$GH_DIR/draft" ;;
    esac ;;
  'pr merge')
    case "$*" in
      *--disable-auto*)
        # Like GitHub: refusing to disable an auto-merge that is not armed.
        [ -f "$GH_DIR/auto" ] || { echo "GraphQL: Can't disable auto-merge for this pull request. (disablePullRequestAutoMerge)" >&2; exit 1; }
        rm -f "$GH_DIR/auto" ;;
      *--auto*) [ ! -f "$GH_DIR/draft" ] || exit 13; touch "$GH_DIR/auto" ;;
    esac ;;
  'pr edit')
    while [ "$#" -gt 0 ]; do
      if [ "$1" = --body-file ]; then cp "$2" "$GH_DIR/submitted-body"; fi
      shift
    done ;;
  'api '*) ;;
esac
if [ "$1" = api ]; then
  case "$*" in
    *'/actions/workflows/ci.yml/runs?'*)
      [ ! -f "$GH_DIR/ci-error" ] || { echo 'CI API denied' >&2; exit 1; }
      if [ -f "$GH_DIR/runs" ]; then cat "$GH_DIR/runs"; exit 0; fi
      head=$(git rev-parse HEAD)
      printf '{"workflow_runs":[{"id":123,"head_sha":"%s","html_url":"https://github.com/project/checkout/actions/runs/123"}]}' "$head" ;;
    *'/actions/runs/123/jobs'*) cat "$GH_DIR/jobs" ;;
    *'--method POST'*|*'--method PATCH'*) echo '{}' ;;
    *) echo '[]' ;;
  esac
fi
"#;

fn setup() -> (World, Options) {
    let world = World::new();
    world.script("gh", GH);
    let body = world.temp.path().join("body.md");
    fs::write(&body, BODY).unwrap();
    jobs(&world, "in_progress", None);
    let options = Options {
        title: Some("Run CI alongside review".into()),
        body_file: Some(body),
        ..Options::default()
    };
    (world, options)
}

fn jobs(world: &World, status: &str, conclusion: Option<&str>) {
    world.answer(
        "jobs",
        &serde_json::json!([{"jobs":[
            {"name":"lint", "status":"completed", "conclusion":"success"},
            {"name":"browser-e2e", "status":status, "conclusion":conclusion}
        ]}])
        .to_string(),
    );
}

fn has(world: &World, name: &str) -> bool {
    world.temp.path().join("gh-answers").join(name).exists()
}

fn remote(world: &World) -> String {
    git::git(&world.root, &["ls-remote", "origin", "refs/heads/feature"])
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn draft_push_precedes_review_then_status_ready_and_auto_merge() {
    let (world, options) = setup();
    let evidence = ship_with(&world.root, &options, &|root, tier, _, task, _| {
        assert_eq!(task, BODY);
        assert_eq!(remote(&world), world.head());
        assert!(has(&world, "draft"));
        let calls = world.calls();
        assert!(
            calls.contains("pr create") && calls.contains("--draft"),
            "{calls}"
        );
        assert!(!calls.contains("context=harness/review"), "{calls}");
        fs::OpenOptions::new()
            .append(true)
            .open(world.temp.path().join("gh.log"))
            .map(|mut f| {
                use std::io::Write;
                writeln!(f, "REVIEW").unwrap();
            })
            .unwrap();
        Ok(graded(root, tier, 9))
    })
    .unwrap();
    let calls = world.calls();
    let create = calls.find("pr create").unwrap();
    let review = calls.find("REVIEW").unwrap();
    let status = calls.find("context=harness/review").unwrap();
    let ready = calls.find("pr ready").unwrap();
    let armed = calls.find("--auto").unwrap();
    assert!(
        create < review && review < status && status < ready && ready < armed,
        "{calls}"
    );
    assert!(!has(&world, "draft") && has(&world, "auto"));
    assert!(
        calls.contains(&format!("head_sha={}", evidence.head)),
        "{calls}"
    );
    let timeline =
        fs::read_to_string(world.root.join(".git/aoe-ship/timeline/feature.jsonl")).unwrap();
    for stage in ["draft-push", "ci-state"] {
        assert!(
            timeline.contains(&format!("\"stage\":\"{stage}\"")),
            "{timeline}"
        );
    }
}

#[test]
fn existing_auto_merge_is_disarmed_before_drafting_and_review_failure_keeps_draft() {
    let (world, options) = setup();
    world.answer("existing", "");
    world.answer("auto", "");
    let error = ship_with(&world.root, &options, &|root, tier, _, _, _| {
        assert!(has(&world, "draft") && !has(&world, "auto"));
        assert_eq!(
            fs::read_to_string(world.temp.path().join("gh-answers/submitted-body")).unwrap(),
            BODY
        );
        Ok(graded(root, tier, 7))
    })
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("review 7/10")
            && error.contains("CI: 1 passed, 1 running, 0 failed (run 123)"),
        "{error}"
    );
    assert!(
        error.contains("draft") && !error.contains("nothing was pushed"),
        "{error}"
    );
    let calls = world.calls();
    assert!(
        calls.find("--disable-auto").unwrap() < calls.find("--undo").unwrap(),
        "{calls}"
    );
    assert!(
        !calls.contains("context=harness/review") && !calls.contains("--auto"),
        "{calls}"
    );
    assert!(has(&world, "draft"));
}

#[test]
fn catch_up_rebase_leases_the_head_written_by_the_draft_push() {
    let (world, options) = setup();
    let original = world.head();
    // Start-of-ship rebase: the draft head already differs from started_head.
    world.move_dev("before.txt", "before\n");
    let draft = std::cell::RefCell::new(String::new());
    let evidence = ship_with(&world.root, &options, &|root, tier, _, _, _| {
        *draft.borrow_mut() = remote(&world);
        assert_eq!(*draft.borrow(), world.head());
        assert_ne!(*draft.borrow(), original);
        let report = graded(root, tier, 9);
        world.move_dev("during.txt", "during\n");
        Ok(report)
    })
    .unwrap();
    assert_ne!(evidence.head, *draft.borrow());
    assert_eq!(remote(&world), evidence.head);
    assert!(evidence.recheck.is_some());
}

#[test]
fn real_ship_cli_reports_running_success_and_failed_ci_exit_with_rerun() {
    for (grade, conclusion, success) in [
        (9, None, true),
        (9, Some("failure"), false),
        (9, Some("success"), true),
        (6, None, false),
    ] {
        let (mut world, options) = setup();
        jobs(
            &world,
            if conclusion.is_some() {
                "completed"
            } else {
                "in_progress"
            },
            conclusion,
        );
        world.script(
            "claude",
            r#"#!/bin/sh
case "$*" in
  *'"grade":'*) answer=$(printf '{"grade":%s,"summary":"scripted review"}' "$CI_REVIEW_GRADE") ;;
  *) answer='{"findings":[],"verdicts":[]}' ;;
esac
printf 'AOE-REVIEW-BEGIN\n%s\nAOE-REVIEW-END\n' "$answer"
"#,
        );
        world.set(
            "SHIP_BODY",
            options.body_file.as_ref().unwrap().to_str().unwrap(),
        );
        world.set("SHIP_TITLE", options.title.as_ref().unwrap());
        world.set("CI_REVIEW_GRADE", &grade.to_string());
        let output = world.cli("ship");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.success(), success, "{text}");
        assert!(text.contains(&format!("review {grade}/10 · CI:")), "{text}");
        assert_eq!(has(&world, "auto"), grade == 9);
        assert_eq!(has(&world, "draft"), grade == 6);
        if grade == 6 {
            assert!(text.contains("remains a draft"), "{text}");
            assert!(!world.calls().contains("context=harness/review"));
        }
        if conclusion == Some("failure") {
            for expected in [
                "browser-e2e",
                "https://github.com/project/checkout/actions/runs/123",
                "gh run rerun 123 --failed",
                "review passed",
                "auto-merge is armed",
                "fix",
                "reship",
            ] {
                assert!(text.contains(expected), "missing {expected}: {text}");
            }
        }
        run(&world.root, &["status", "--porcelain"]);
    }
}

#[test]
fn carry_does_not_publish_a_review_status_on_a_draft() {
    let world = World::new();
    world.answer(
        "pull-1.json",
        &serde_json::json!({
            "state":"open", "draft":true, "base":{"ref":"dev"}, "head":{"sha":world.head()}
        })
        .to_string(),
    );
    crate::ship::postpush::carry::carry(&world.root, 1).unwrap();
    assert!(!world.calls().contains("context=harness/review"));
    assert!(!world.calls().contains("/comments"));
}

#[test]
fn already_draft_reship_is_idempotent_and_still_reports_ci_on_review_refusal() {
    let (world, options) = setup();
    world.answer("existing", "");
    world.answer("draft", "");
    ship_with(&world.root, &options, &|root, tier, _, _, _| {
        Ok(graded(root, tier, 7))
    })
    .unwrap_err();
    let error = ship_with(&world.root, &options, &|_, _, _, _, _| {
        panic!("failed commit is not reviewed again")
    })
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("already failed") && error.contains("review 7/10 · CI:"),
        "{error}"
    );
    assert!(has(&world, "draft") && !has(&world, "auto"));
    assert!(!world.calls().contains("--undo"));
}

#[test]
fn draft_and_reviewer_use_the_validated_body_even_when_the_input_file_changes_during_gates() {
    let (world, options) = setup();
    world.answer("existing", "");
    fs::write(
        world.root.join("Makefile"),
        format!(
            "fmt-check:\n\t@printf changed > '{}'\n",
            options.body_file.as_ref().unwrap().display()
        ),
    )
    .unwrap();
    run(
        &world.root,
        &[
            "commit",
            "-q",
            "-am",
            "gate changes external description input",
        ],
    );
    let error = ship_with(&world.root, &options, &|root, tier, _, task, _| {
        assert_eq!(
            fs::read_to_string(options.body_file.as_ref().unwrap()).unwrap(),
            "changed"
        );
        assert_eq!(task, BODY);
        assert_eq!(
            fs::read_to_string(world.temp.path().join("gh-answers/submitted-body")).unwrap(),
            BODY
        );
        Ok(graded(root, tier, 7))
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("review 7/10"), "{error}");
}

#[test]
fn ci_uses_latest_run_for_exact_head_and_counts_every_jobs_page() {
    let (world, _) = setup();
    world.answer("runs", &serde_json::json!({"workflow_runs":[
        {"id":999,"head_sha":"another head","html_url":"wrong"},
        {"id":122,"head_sha":world.head(),"html_url":"obsolete"},
        {"id":123,"head_sha":world.head(),"html_url":"https://github.com/project/checkout/actions/runs/123"}
    ]}).to_string());
    world.answer(
        "jobs",
        r#"[{"jobs":[{"name":"lint","status":"completed","conclusion":"success"}]},
        {"jobs":[{"name":"e2e","status":"completed","conclusion":"timed_out"},
        {"name":"unit","status":"queued","conclusion":null}]}]"#,
    );
    let error = crate::ship::postpush::ci::outcome(&world.root, &world.head(), Some(9), Ok(()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("CI: 1 passed, 1 running, 1 failed (run 123)"),
        "{error}"
    );
    assert!(error.contains("Failed jobs: e2e"), "{error}");
    assert!(!world.calls().contains("/runs/999/jobs") && !world.calls().contains("/runs/122/jobs"));
}

#[test]
fn missing_ci_run_is_pending_and_api_failure_preserves_review_failure() {
    let (world, _) = setup();
    world.answer("runs", r#"{"workflow_runs":[]}"#);
    crate::ship::postpush::ci::outcome(&world.root, &world.head(), Some(9), Ok(())).unwrap();
    let error = crate::ship::postpush::ci::outcome::<()>(
        &world.root,
        &world.head(),
        Some(7),
        Err("review failed; nothing was pushed".into()),
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("CI: not started yet") && error.contains("remains a draft"),
        "{error}"
    );
    world.answer("ci-error", "");
    let error = crate::ship::postpush::ci::outcome::<()>(
        &world.root,
        &world.head(),
        Some(7),
        Err("review failed".into()),
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("review failed")
            && error.contains("review 7/10 · CI: unavailable")
            && error.contains("CI API denied"),
        "{error}"
    );
}

#[test]
fn ownlease_real_ship_cli_reports_queued_and_not_started_ci() {
    for (force, runs, expected) in [
        ("0", None, "CI: queued (run 123)"),
        ("1", None, "CI: queued (run 123)"),
        ("0", Some("{\"workflow_runs\":[]}"), "CI: not started yet"),
        ("1", Some("{\"workflow_runs\":[]}"), "CI: not started yet"),
    ] {
        let (mut world, options) = setup();
        world.answer("jobs", r#"[{"jobs":[]}]"#);
        if let Some(runs) = runs {
            world.answer("runs", runs);
        }
        // Origin holds two of our three commits on an older dev.
        fs::write(world.root.join("second.txt"), "second\n").unwrap();
        run(&world.root, &["add", "second.txt"]);
        run(&world.root, &["commit", "-q", "-m", "second"]);
        run(&world.root, &["push", "-q", "origin", "feature"]);
        fs::write(world.root.join("third.txt"), "third\n").unwrap();
        run(&world.root, &["add", "third.txt"]);
        run(&world.root, &["commit", "-q", "-m", "third"]);
        world.move_dev("dev-only.txt", "dev advanced\n");
        world.rebase_onto_dev();
        world.script(
            "claude",
            r#"#!/bin/sh
case "$*" in
  *'"grade":'*) answer='{"grade":10,"summary":"scripted review"}' ;;
  *) answer='{"findings":[],"verdicts":[]}' ;;
esac
printf 'AOE-REVIEW-BEGIN\n%s\nAOE-REVIEW-END\n' "$answer"
"#,
        );
        world.set(
            "SHIP_BODY",
            options.body_file.as_ref().unwrap().to_str().unwrap(),
        );
        world.set("SHIP_TITLE", options.title.as_ref().unwrap());
        world.set("SHIP_FORCE", force);
        let output = world.cli("ship");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.status.success(), "{text}");
        assert!(
            force == "1"
                || text.contains(
                    "ship: origin/feature held an earlier push of the same patch; replacing it"
                ),
            "{text}"
        );
        assert!(
            text.contains(&format!("ship: review 10/10 · {expected}")),
            "{text}"
        );
        assert_eq!(remote(&world), world.head());
        assert!(has(&world, "auto") && !has(&world, "draft"));
    }
}

#[test]
fn a_failed_ci_workflow_without_jobs_is_still_a_failure() {
    let (world, _) = setup();
    world.answer("runs", &serde_json::json!({"workflow_runs":[{
        "id":123, "head_sha":world.head(), "html_url":"https://github.com/project/checkout/actions/runs/123",
        "conclusion":"failure"
    }]}).to_string());
    world.answer("jobs", r#"[{"jobs":[]}]"#);
    let error = crate::ship::postpush::ci::outcome(&world.root, &world.head(), Some(10), Ok(()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("CI: 0 passed, 0 running, 1 failed (run 123)"),
        "{error}"
    );
    assert!(error.contains("CI workflow (failure)"), "{error}");
    assert!(!error.contains("CI: queued"), "{error}");
}

#[test]
fn pull_request_state_parses_githubs_json_for_drafts_and_armed_auto_merge() {
    use crate::ship::postpush::draft::PrState;
    // A failed review leaves exactly this: a draft with no auto-merge (#518, 2026-10-21).
    let draft = PrState::parse(r#"{"isDraft":true,"autoMergeRequest":null}"#).unwrap();
    assert!(draft.is_draft && !draft.armed());
    let armed = PrState::parse(
        r#"{"isDraft":false,"autoMergeRequest":{"enabledAt":"2026-10-21T00:00:00Z","mergeMethod":"SQUASH"}}"#,
    )
    .unwrap();
    assert!(!armed.is_draft && armed.armed());
    assert!(
        PrState::parse("true false").is_err(),
        "jq-style text is not accepted"
    );
}
