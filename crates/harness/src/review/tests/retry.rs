use super::super::runner::ask_with;
use std::{fs, process::Command, time::Duration};

fn ask(messages: &[(&str, &str, i32)]) -> (Result<String, String>, Vec<Duration>, usize) {
    let temp = tempfile::tempdir().unwrap();
    let mut calls = 0;
    let mut waits = Vec::new();
    let result = ask_with(
        || {
            let (stdout, stderr, code) = messages[calls.min(messages.len() - 1)];
            calls += 1;
            fs::write(temp.path().join("out"), stdout).unwrap();
            fs::write(temp.path().join("err"), stderr).unwrap();
            let mut command = Command::new("sh");
            command.current_dir(temp.path()).args([
                "-c",
                "cat out; cat err >&2; exit \"$1\"",
                "reviewer-stub",
                &code.to_string(),
            ]);
            Ok((command, None))
        },
        |delay| waits.push(delay),
    );
    (result, waits, calls)
}

const REPLY: &str = "AOE-REVIEW-BEGIN\n{\"findings\": []}\nAOE-REVIEW-END";

#[test]
fn reviewer_rate_limits_retry_the_process_with_bounded_backoff() {
    for message in [
        "exceeded retry limit, last status: 429 Too Many Requests",
        "HTTP 429",
        "Too Many Requests",
        "rate limit exceeded",
        "Rate-limited",
        "model at capacity",
        "usage limit",
        "status 429",
        r#"{"error": {"type": "rate_limit_exceeded"}}"#,
    ] {
        for stdout in [true, false] {
            let failure = if stdout {
                (message, "", 1)
            } else {
                ("", message, 1)
            };
            let (result, waits, calls) = ask(&[failure, failure, (REPLY, "", 0)]);
            assert_eq!(result.as_deref(), Ok("{\"findings\": []}"), "{message}");
            assert_eq!(calls, 3);
            assert_eq!(waits, [Duration::from_secs(60), Duration::from_secs(180)]);
        }
    }
}

#[test]
fn exhausted_reviewer_rate_limits_explain_how_to_reship() {
    let (result, waits, calls) = ask(&[("", "HTTP 429", 1)]);
    let error = result.unwrap_err();
    assert!(error.contains("rate-limited"), "{error}");
    assert!(
        error.contains("wait and ship again; nothing needs changing"),
        "{error}"
    );
    assert_eq!(calls, 3);
    assert_eq!(waits.len(), 2);
}

#[test]
fn other_reviewer_failures_and_successful_answers_are_not_retried() {
    for message in [
        "line 429 failed",
        "src/foo.rs:429:12",
        "request id 429",
        "429 bytes",
        "http://429",
        "HTTP 4290",
        "429",
        "rate limiter crashed",
        "prorate limits",
        "permission denied",
    ] {
        let (result, waits, calls) = ask(&[("", message, 1)]);
        assert!(result.is_err());
        assert_eq!(calls, 1, "{message}");
        assert!(waits.is_empty());
    }
    let (result, waits, calls) = ask(&[(REPLY, "HTTP 429", 0)]);
    assert!(result.is_ok());
    assert_eq!(calls, 1);
    assert!(waits.is_empty());
    let (_, waits, calls) = ask(&[("", "HTTP 429", 1), ("", "permission denied", 1)]);
    assert_eq!(calls, 2);
    assert_eq!(waits.len(), 1);
}

#[test]
fn fallback_uses_other_runtime_once_after_backoff() {
    use super::super::runner::ask_fallback_with;
    use crate::agents::Runtime;
    let mut calls = Vec::new();
    let mut waits = Vec::new();
    let (answer, fallback) = ask_fallback_with(
        Runtime::Codex,
        |runtime| {
            calls.push(runtime);
            let mut command = Command::new("sh");
            if runtime == Runtime::Codex {
                command.args(["-c", "echo 'at capacity' >&2; exit 1"]);
            } else {
                command.args([
                    "-c",
                    "printf 'AOE-REVIEW-BEGIN\\n{\"findings\": []}\\nAOE-REVIEW-END\\n'",
                ]);
            }
            Ok((command, None))
        },
        |delay| waits.push(delay),
    );
    assert_eq!(answer.as_deref(), Ok("{\"findings\": []}"));
    assert!(fallback);
    assert_eq!(
        calls,
        [
            Runtime::Codex,
            Runtime::Codex,
            Runtime::Codex,
            Runtime::Claude
        ]
    );
    assert_eq!(waits, [Duration::from_secs(60), Duration::from_secs(180)]);
}

#[test]
fn fallback_failure_stays_an_error() {
    use super::super::runner::ask_fallback_with;
    use crate::agents::Runtime;
    let (answer, fallback) = ask_fallback_with(
        Runtime::Claude,
        |_| {
            let mut command = Command::new("sh");
            command.args(["-c", "echo 'usage limit' >&2; exit 1"]);
            Ok((command, None))
        },
        |_| {},
    );
    assert!(fallback);
    assert!(answer.is_err());
}

use super::super::{Ask, Plan, Tier, report, review_with};
use super::flow::fixture;
use crate::agents::Runtime;

struct FallbackScript {
    failed: bool,
    used: std::sync::atomic::AtomicBool,
}

impl Ask for FallbackScript {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        prompts.into_iter().map(|prompt| {
            if prompt.contains("# Grader") {
                return Ok(r#"{"grade": 9, "summary": "ok"}"#.into());
            }
            let (result, used) = super::super::runner::ask_fallback_with(
                Runtime::Codex,
                |runtime| {
                    let mut command = Command::new("sh");
                    if runtime == Runtime::Codex || self.failed {
                        command.args(["-c", "echo 'model at capacity' >&2; exit 1"]);
                    } else {
                        command.args(["-c", "printf 'AOE-REVIEW-BEGIN\\n{\"findings\": []}\\nAOE-REVIEW-END\\n'"]);
                    }
                    Ok((command, None))
                },
                |_| {},
            );
            self.used.store(used, std::sync::atomic::Ordering::Relaxed);
            result
        }).collect()
    }

    fn runtime_fallback(&self) -> bool {
        self.used.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[test]
fn fallback_is_recorded_and_review_is_complete_only_if_it_answers() {
    for failed in [false, true] {
        let temp = fixture("README.md");
        let script = FallbackScript {
            failed,
            used: std::sync::atomic::AtomicBool::new(false),
        };
        let report = review_with(
            temp.path(),
            Tier::Low,
            Runtime::Codex,
            "",
            &Plan::Full,
            &script,
        )
        .unwrap();
        assert!(report.runtime_fallback);
        assert_eq!(report.complete(), !failed);
        assert_eq!(report.passes(), !failed);
        let stored = std::fs::read_to_string(
            report::directory(temp.path())
                .unwrap()
                .read_dir()
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .unwrap();
        assert!(stored.contains("\"runtime_fallback\": true"));
    }
}

#[test]
fn live_entry_uses_fallback_models_and_records_the_switch() {
    use std::os::unix::fs::PermissionsExt;
    const CHILD: &str = "AOE_REVIEW_LIVE_TEST_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::Path::new(&root);
        let branch = crate::ship::git::branch(root).unwrap_or_default();
        let report = super::super::entry::review_judged(
            root,
            &branch,
            None,
            Tier::High,
            Runtime::Codex,
            "scripted review",
            &crate::review::Plan::Full,
        )
        .unwrap();
        assert!(report.complete(), "{:?}", report.failures);
        assert!(report.runtime_fallback);
        assert_eq!(
            report.findings[0].status,
            super::super::protocol::Status::Confirmed
        );
        assert_eq!(
            report.findings[0].votes.last().unwrap().reviewer,
            super::super::protocol::ARBITER
        );
        println!(
            "LIVE_TEST: complete={}, runtime_fallback={}, arbiter=confirmed",
            report.complete(),
            report.runtime_fallback
        );
        return;
    }
    let temp = fixture("README.md");
    fs::create_dir(temp.path().join(".codex")).unwrap();
    fs::write(temp.path().join(".codex/hooks.json"), r#"{"hooks":{}}"#).unwrap();
    super::flow::git(temp.path(), &["add", ".codex/hooks.json"]);
    super::flow::git(
        temp.path(),
        &["commit", "-q", "-m", "configure scripted runner"],
    );
    let scripts_dir = tempfile::tempdir().unwrap();
    let bin = scripts_dir.path();
    let log = bin.join("models.log");
    let scripts = [
        (
            "codex",
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$AOE_REVIEW_LIVE_TEST_LOG"
echo 'HTTP 429' >&2
exit 1
"#,
        ),
        (
            "claude",
            r##"#!/bin/sh
printf '%s\n' "$*" >> "$AOE_REVIEW_LIVE_TEST_LOG"
case "$*" in
  *'# Arbiter'*) answer='{"rulings":[{"id":"F1","ruling":"upheld","evidence":"README.md:1"}]}' ;;
  *'# Grader'*) answer='{"grade":9,"summary":"ok"}' ;;
  *'# Spec and scope auditor'*"verify the other reviewers' findings"*) answer='{"verdicts":[{"id":"F1","verdict":"upheld","evidence":"README.md:1"}],"findings":[]}' ;;
  *'# Hostile-input and safety adversary'*"verify the other reviewers' findings"*) answer='{"verdicts":[{"id":"F1","verdict":"refuted","evidence":"README.md:1"}],"findings":[]}' ;;
  *"verify the other reviewers' findings"*) answer='{"verdicts":[],"findings":[]}' ;;
  *'# Correctness breaker'*) answer='{"findings":[{"file":"README.md","line":1,"severity":"major","category":"correctness","claim":"wrong text","trigger":"read it","expected_vs_actual":"base vs changed","evidence":"README.md:1"}]}' ;;
  *) answer='{"findings":[]}' ;;
esac
printf 'AOE-REVIEW-BEGIN\n%s\nAOE-REVIEW-END\n' "$answer"
"##,
        ),
    ];
    for (name, script) in scripts {
        let path = bin.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "review::tests::retry::live_entry_uses_fallback_models_and_records_the_switch",
            "--nocapture",
        ])
        .env(CHILD, temp.path())
        .env("PATH", path)
        .env("AOE_REVIEW_LIVE_TEST_LOG", &log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let models = fs::read_to_string(log).unwrap();
    assert!(models.contains("-m gpt-6.1-terra"), "codex model: {models}");
    assert!(
        models.contains("--model claude-sonnet-5-5"),
        "average fallback: {models}"
    );
    assert!(
        models.contains("--model claude-opus-5-5"),
        "arbiter fallback: {models}"
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
