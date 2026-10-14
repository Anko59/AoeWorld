use crate::ship::git::remote::output_with;
use std::{process::Command, time::Duration};

mod paths;

fn run(errors: &[&str], operation: &str) -> (Result<String, String>, Vec<Duration>, usize) {
    let temp = tempfile::tempdir().unwrap();
    let counter = temp.path().join("attempts");
    let mut command = Command::new("sh");
    command.env("SSH_AUTH_SOCK", "/stub/agent").args([
        "-c",
        "echo \"$4\" >> \"$1\"; n=$(wc -l < \"$1\"); if [ \"$n\" = 1 ]; then msg=$2; else msg=$3; fi; if [ -n \"$msg\" ]; then echo \"$msg\" >&2; exit 128; fi; echo recovered",
        "git-stub",
        counter.to_str().unwrap(),
        errors[0],
        errors.get(1).copied().unwrap_or(errors[0]),
        operation,
    ]);
    let mut waits = Vec::new();
    let result = output_with(&mut command, |delay| waits.push(delay))
        .map(|out| String::from_utf8(out.stdout).unwrap());
    let calls = std::fs::read_to_string(counter).unwrap().lines().count();
    (result, waits, calls)
}

#[test]
fn fetch_and_push_retry_once_only_on_the_exact_publickey_error() {
    for operation in ["fetch", "push"] {
        let (result, waits, calls) = run(&["Permission denied (publickey)", ""], operation);
        assert_eq!(result.as_deref(), Ok("recovered\n"));
        assert_eq!(calls, 2);
        assert_eq!(waits, [Duration::from_secs(20)]);
        for error in [
            "Permission denied",
            "Permission denied (password)",
            "connection refused",
            "non-fast-forward",
        ] {
            let (result, waits, calls) = run(&[error], operation);
            assert!(result.is_err());
            assert_eq!(calls, 1, "{error}");
            assert!(waits.is_empty());
            assert!(!result.unwrap_err().contains("served no key"));
        }
    }
}

#[test]
fn exhausted_publickey_errors_name_the_agent_and_explicit_key_workaround() {
    let (result, waits, calls) = run(&["Permission denied (publickey)"], "fetch");
    let error = result.unwrap_err();
    assert_eq!(calls, 2);
    assert_eq!(waits, [Duration::from_secs(20)]);
    assert!(error.contains("Permission denied (publickey)"), "{error}");
    assert!(
        error.contains("SSH_AUTH_SOCK")
            && error.contains("/stub/agent")
            && error.contains("served no key"),
        "{error}"
    );
    assert!(
        error.contains("GIT_SSH_COMMAND='ssh -i <key> -o IdentitiesOnly=yes'"),
        "{error}"
    );
}

#[test]
fn a_different_error_on_the_second_attempt_is_reported_without_agent_advice() {
    let (result, waits, calls) = run(
        &["Permission denied (publickey)", "non-fast-forward"],
        "push",
    );
    assert_eq!(calls, 2);
    assert_eq!(waits.len(), 1);
    let error = result.unwrap_err();
    assert!(error.contains("non-fast-forward"));
    assert!(!error.contains("served no key"));
}

#[test]
fn a_failed_push_keeps_pre_push_output_from_both_streams() {
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "echo 'pre-push gate details'; echo 'gate failed' >&2; exit 1",
    ]);
    let error = output_with(&mut command, |_| panic!("ordinary failures never retry")).unwrap_err();
    assert!(error.contains("pre-push gate details"), "{error}");
    assert!(error.contains("gate failed"), "{error}");
}
