use super::*;
use crate::process::{Cancellation, capture_in};
use std::{fs, path::Path, thread, time::Duration};

fn shell(root: &Path, script: &str, budget: Duration, cancel: &Cancellation) -> Captured {
    capture_in(root, "sh", &["-c", script], &[], budget, cancel)
}

fn measured(captured: &Captured) -> SafeObservation {
    let observation = safe_observation(captured);
    assert_eq!(observation.schema, 1);
    assert_eq!(observation.root_cause, RootCause::NotAssessed);
    assert_eq!(observation.stdout.bytes, captured.stdout.len());
    assert_eq!(observation.stderr.bytes, captured.stderr.len());
    assert_eq!(
        observation.stdout.raw_blake3,
        blake3::hash(&captured.stdout).to_hex().to_string()
    );
    assert_eq!(
        observation.stderr.raw_blake3,
        blake3::hash(&captured.stderr).to_hex().to_string()
    );
    assert_eq!(observation.truncated, captured.truncated);
    assert_eq!(observation.duration_ms, captured.duration.as_millis());
    let value = serde_json::to_value(&observation).unwrap();
    assert_eq!(value["root_cause"], "ROOT_CAUSE_NOT_ASSESSED");
    observation
}

#[test]
fn actual_failure_metadata_does_not_classify_or_expose_raw_text_and_survives_later_success() {
    let root = tempfile::tempdir().unwrap();
    let failed = shell(
        root.path(),
        "printf 'SECRET-PROCESS-OBSERVATION-v1 panic stacktrace'; printf 'SECRET-PROCESS-OBSERVATION-v1 credentials=/private/secret' >&2; exit 7",
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    assert!(matches!(failed.exit, CaptureExit::Failed(Some(7))));
    let first = measured(&failed);
    assert_eq!(
        first.outcome,
        Outcome::Failed {
            code: Some(7),
            termination: Termination::ExitCode
        }
    );
    assert_eq!(first.outcome.exit_label(), "FAILED");
    assert_eq!(
        first.stdout.bytes,
        b"SECRET-PROCESS-OBSERVATION-v1 panic stacktrace".len()
    );
    assert_eq!(
        first.stderr.bytes,
        b"SECRET-PROCESS-OBSERVATION-v1 credentials=/private/secret".len()
    );
    let encoded = serde_json::to_string(&first).unwrap();
    for raw in [
        "SECRET-PROCESS-OBSERVATION-v1",
        "stacktrace",
        "credentials",
        "/private/secret",
        "CODE_BUG",
    ] {
        assert!(!encoded.contains(raw));
    }
    let succeeded = shell(
        root.path(),
        "printf normal; printf diagnostic >&2",
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    assert!(matches!(succeeded.exit, CaptureExit::Success));
    let second = measured(&succeeded);
    assert_eq!(second.outcome, Outcome::Success);
    assert_eq!(second.outcome.exit_label(), "SUCCESS");
    assert_eq!(second.stdout.bytes, 6);
    assert_eq!(second.stderr.bytes, 10);
    assert_eq!(
        measured(&failed),
        first,
        "an independent success cannot rewrite the first failure"
    );
    assert_ne!(first, second);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn actual_missing_executable_start_metadata_omits_private_path_and_error_message() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("SECRET-MISSING-PROGRAM-v1");
    let captured = capture_in(
        root.path(),
        missing.to_str().unwrap(),
        &[],
        &[],
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Start(_)));
    let observation = measured(&captured);
    assert_eq!(
        observation.outcome,
        Outcome::Start {
            io_kind: SafeErrorKind::NotFound
        }
    );
    assert_eq!(observation.outcome.exit_label(), "START_UNAVAILABLE");
    assert_eq!(observation.stdout.bytes, 0);
    assert_eq!(observation.stderr.bytes, 0);
    let encoded = serde_json::to_string(&observation).unwrap();
    assert!(!encoded.contains("SECRET-MISSING-PROGRAM-v1"));
    assert!(!encoded.contains(root.path().to_str().unwrap()));
}

#[cfg(unix)]
#[test]
fn actual_signalled_child_without_exit_code_is_signal_or_unknown_not_a_diagnosis() {
    let root = tempfile::tempdir().unwrap();
    let captured = shell(
        root.path(),
        "kill -TERM $$",
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Failed(None)));
    let observation = measured(&captured);
    assert_eq!(
        observation.outcome,
        Outcome::Failed {
            code: None,
            termination: Termination::SignalOrUnknown
        }
    );
    assert_eq!(observation.outcome.exit_label(), "FAILED");
    let encoded = serde_json::to_value(&observation).unwrap();
    assert_eq!(encoded["outcome"]["code"], serde_json::Value::Null);
    assert_eq!(encoded["outcome"]["termination"], "SIGNAL_OR_UNKNOWN");
}

#[test]
fn actual_pre_admission_cancel_and_zero_budget_keep_empty_tails_without_spawning() {
    let root = tempfile::tempdir().unwrap();
    let cancel = Cancellation::default();
    cancel.cancel();
    let cancelled = shell(
        root.path(),
        "printf spawned > marker",
        Duration::from_secs(2),
        &cancel,
    );
    let observation = measured(&cancelled);
    assert_eq!(observation.outcome, Outcome::Cancelled);
    assert_eq!(observation.outcome.exit_label(), "CANCELLED");
    assert_eq!((observation.stdout.bytes, observation.stderr.bytes), (0, 0));
    assert!(!root.path().join("marker").exists());
    let expired = shell(
        root.path(),
        "printf spawned > marker",
        Duration::ZERO,
        &Cancellation::default(),
    );
    let observation = measured(&expired);
    assert_eq!(observation.outcome, Outcome::Deadline);
    assert_eq!(observation.outcome.exit_label(), "DEADLINE");
    assert_eq!((observation.stdout.bytes, observation.stderr.bytes), (0, 0));
    assert!(!root.path().join("marker").exists());
}

#[test]
fn actual_deadline_and_active_cancellation_preserve_partial_both_stream_metadata() {
    let root = tempfile::tempdir().unwrap();
    let expired = shell(
        root.path(),
        "printf before; printf diagnostic >&2; sleep 5",
        Duration::from_millis(200),
        &Cancellation::default(),
    );
    let observation = measured(&expired);
    assert_eq!(observation.outcome, Outcome::Deadline);
    assert_eq!(expired.stdout, b"before");
    assert_eq!(expired.stderr, b"diagnostic");
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        signal.cancel();
    });
    let cancelled = shell(
        root.path(),
        "printf partial; printf diagnostic >&2; sleep 5",
        Duration::from_secs(5),
        &cancel,
    );
    worker.join().unwrap();
    let observation = measured(&cancelled);
    assert_eq!(observation.outcome, Outcome::Cancelled);
    assert_eq!(cancelled.stdout, b"partial");
    assert_eq!(cancelled.stderr, b"diagnostic");
}

#[test]
fn actual_two_stream_overflow_observes_retained_tails_not_total_output() {
    let root = tempfile::tempdir().unwrap();
    let captured = shell(
        root.path(),
        "/usr/bin/head -c 70000 /dev/zero; /usr/bin/head -c 70000 /dev/zero >&2",
        Duration::from_secs(2),
        &Cancellation::default(),
    );
    assert!(matches!(captured.exit, CaptureExit::Success));
    let observation = measured(&captured);
    assert!(observation.truncated);
    assert_eq!(observation.stdout.bytes, crate::process::LOG_LIMIT);
    assert_eq!(observation.stderr.bytes, crate::process::LOG_LIMIT);
    assert_ne!(observation.stdout.bytes, 70000);
    assert_eq!(observation.stdout.raw_blake3, observation.stderr.raw_blake3);
}

#[test]
fn synthetic_monitor_error_kind_mapping_never_serializes_error_message_or_invents_runtime_proof() {
    // Mapping-only synthetic cases, not proof of an actual monitor runtime failure.
    let kinds = [
        (io::ErrorKind::NotFound, SafeErrorKind::NotFound),
        (
            io::ErrorKind::PermissionDenied,
            SafeErrorKind::PermissionDenied,
        ),
        (io::ErrorKind::TimedOut, SafeErrorKind::TimedOut),
        (io::ErrorKind::Interrupted, SafeErrorKind::Interrupted),
        (io::ErrorKind::InvalidData, SafeErrorKind::InvalidData),
        (io::ErrorKind::InvalidInput, SafeErrorKind::InvalidInput),
        (io::ErrorKind::Unsupported, SafeErrorKind::Unsupported),
        (io::ErrorKind::WouldBlock, SafeErrorKind::WouldBlock),
        (io::ErrorKind::AlreadyExists, SafeErrorKind::AlreadyExists),
        (
            io::ErrorKind::ConnectionRefused,
            SafeErrorKind::ConnectionRefused,
        ),
        (
            io::ErrorKind::ConnectionReset,
            SafeErrorKind::ConnectionReset,
        ),
        (
            io::ErrorKind::ConnectionAborted,
            SafeErrorKind::ConnectionAborted,
        ),
        (io::ErrorKind::NotConnected, SafeErrorKind::NotConnected),
        (io::ErrorKind::BrokenPipe, SafeErrorKind::BrokenPipe),
        (io::ErrorKind::UnexpectedEof, SafeErrorKind::UnexpectedEof),
        (io::ErrorKind::AddrInUse, SafeErrorKind::AddrInUse),
        (
            io::ErrorKind::AddrNotAvailable,
            SafeErrorKind::AddrNotAvailable,
        ),
        (io::ErrorKind::WriteZero, SafeErrorKind::WriteZero),
        (io::ErrorKind::OutOfMemory, SafeErrorKind::OutOfMemory),
        (io::ErrorKind::Other, SafeErrorKind::OtherUnknown),
        (
            io::ErrorKind::DirectoryNotEmpty,
            SafeErrorKind::OtherUnknown,
        ),
    ];
    for (kind, expected) in kinds {
        let captured = Captured {
            exit: CaptureExit::Monitor(io::Error::new(
                kind,
                "SECRET-IO-ERROR-v1 /private/credentials",
            )),
            stdout: Vec::new(),
            stderr: Vec::new(),
            truncated: true,
            duration: Duration::from_nanos(1234567),
        };
        let observation = measured(&captured);
        assert_eq!(observation.outcome, Outcome::Monitor { io_kind: expected });
        assert_eq!(observation.outcome.exit_label(), "MONITOR_UNAVAILABLE");
        assert!(
            observation.truncated,
            "small empty tails can still mean incomplete EOF"
        );
        assert_eq!(observation.duration_ms, 1);
        let encoded = serde_json::to_string(&observation).unwrap();
        assert!(!encoded.contains("SECRET-IO-ERROR-v1"));
        assert!(!encoded.contains("/private/credentials"));
    }
}
