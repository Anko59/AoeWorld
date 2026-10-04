//! Copy to crates/harness/src/gates/policy/backend/tests/receipts.rs;
//! add `mod receipts;` to existing backend/tests.rs. No production DI needed.
use super::*;

#[cfg(unix)]
#[test]
fn bounded_capture_retains_success_failure_deadline_cancellation_start_and_truncation_receipts() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let output = PrivateOutput::new(external.path(), &[root.path().to_owned()]).unwrap();
    let cancellation = Cancellation::default();
    let bytes = capture(
        root.path(),
        "/bin/sh",
        &["-c", "printf 'raw\\000bytes'; printf 'diagnostic\\377' >&2"],
        "receipt-success.log",
        &output,
        &cancellation,
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(bytes, b"raw\0bytes");
    let receipt = fs::read(external.path().join("receipt-success.log")).unwrap();
    assert!(
        receipt
            .windows(b"raw\0bytes".len())
            .any(|window| window == b"raw\0bytes")
    );
    assert!(
        receipt
            .windows(b"diagnostic\xff".len())
            .any(|window| window == b"diagnostic\xff")
    );
    for (name, program, args, deadline, cancelled, reason) in [
        (
            "receipt-failed.log",
            "/bin/sh",
            vec!["-c", "printf partial-out; printf partial-err >&2; exit 7"],
            Duration::from_secs(1),
            false,
            "failed",
        ),
        (
            "receipt-deadline.log",
            "/bin/sh",
            vec!["-c", "printf SHOULD_NOT_RUN"],
            Duration::ZERO,
            false,
            "deadline",
        ),
        (
            "receipt-cancel.log",
            "/bin/sh",
            vec!["-c", "printf SHOULD_NOT_RUN"],
            Duration::from_secs(1),
            true,
            "cancelled",
        ),
        (
            "receipt-start.log",
            "/definitely-missing-policy-fixture-executable",
            vec![],
            Duration::from_secs(1),
            false,
            "process error",
        ),
        (
            "receipt-truncated.log",
            "/bin/sh",
            vec!["-c", "printf '%070000d' 0; printf retained-tail"],
            Duration::from_secs(1),
            false,
            "truncated",
        ),
    ] {
        let cancellation = Cancellation::default();
        if cancelled {
            cancellation.cancel();
        }
        let error = capture(
            root.path(),
            program,
            &args,
            name,
            &output,
            &cancellation,
            deadline,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(reason), "{name}: {error}");
        let path = external.path().join(name);
        let log = fs::read(&path).unwrap();
        assert!(log.starts_with(b"--- stdout ---\n"));
        assert!(
            log.windows(b"--- stderr ---".len())
                .any(|window| window == b"--- stderr ---")
        );
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(
            !log.windows(b"SHOULD_NOT_RUN".len())
                .any(|window| window == b"SHOULD_NOT_RUN")
        );
        if name == "receipt-failed.log" {
            assert!(
                log.windows(b"partial-err".len())
                    .any(|window| window == b"partial-err")
            );
        }
        if name == "receipt-truncated.log" {
            assert!(log.ends_with(b"\n--- stderr ---\n"));
            assert!(
                log.windows(b"retained-tail".len())
                    .any(|window| window == b"retained-tail")
            );
        }
    }
}
#[test]
fn api_observation_shape_errors_are_not_equivalent_to_absent_protection() {
    let (repo, branch, protection) = responses();
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"id":"7","full_name":"Example/Policy"}),
        serde_json::json!({"id":7,"full_name":17}),
    ] {
        assert!(observed(&anchor(), &bad, &branch, &protection).is_err());
    }
    for bad in [
        serde_json::json!({"name":"dev","protected":true}),
        serde_json::json!({"name":"dev","protected":"true","commit":{"sha":"1".repeat(40)}}),
        serde_json::json!({"name":"dev","protected":true,"commit":{"sha":17}}),
    ] {
        assert!(observed(&anchor(), &repo, &bad, &protection).is_err());
    }
    for required in [
        serde_json::json!({"strict":"true","contexts":["required"]}),
        serde_json::json!({"strict":true,"contexts":[17]}),
        serde_json::json!({"strict":true,"checks":"required"}),
        serde_json::json!({"strict":true,"checks":[{}]}),
    ] {
        assert!(
            observed(
                &anchor(),
                &repo,
                &branch,
                &serde_json::json!({"required_status_checks":required})
            )
            .is_err()
        );
    }
    let checks_only = serde_json::json!({"required_status_checks":{"strict":true,"checks":[{"context":"required","app_id":12}]}});
    let source = observed(&anchor(), &repo, &branch, &checks_only).unwrap();
    assert!(source.required_contexts.contains("required"));
}
