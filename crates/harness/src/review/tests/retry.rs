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
