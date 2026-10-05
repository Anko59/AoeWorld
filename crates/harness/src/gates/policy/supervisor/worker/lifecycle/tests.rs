//! Finite traces verify wiring, not production Docker behavior.
#![cfg(unix)]
use super::super::{
    Phase,
    tests::{binding, cid, image, inspect},
};
use super::*;
use std::collections::VecDeque;
struct Trace {
    replies: VecDeque<(Phase, Captured)>,
    now: Duration,
    calls: Vec<Phase>,
}
impl Backend for Trace {
    fn now(&self) -> Duration {
        self.now
    }
    fn call(&mut self, _: &Binding, action: &Action, _: Duration, _: &Cancellation) -> Captured {
        self.now += Duration::from_millis(1);
        self.calls.push(action.phase());
        let (phase, captured) = self.replies.pop_front().expect("unexpected action");
        assert_eq!(phase, action.phase());
        captured
    }
}
#[derive(Default)]
struct IntentTrace {
    calls: usize,
    fail: Option<usize>,
}
impl Intent for IntentTrace {
    fn append(&mut self, _: Event, _: Option<&Cid>) -> Result<(), &'static str> {
        self.calls += 1;
        if self.fail == Some(self.calls) {
            Err("injected journal failure")
        } else {
            Ok(())
        }
    }
    fn verify(&mut self) -> Result<(), &'static str> {
        Ok(())
    }
}
fn captured(bytes: Vec<u8>, exit: CaptureExit) -> Captured {
    Captured {
        exit,
        stdout: bytes,
        stderr: vec![],
        truncated: false,
        duration: Duration::from_millis(1),
    }
}
fn ok(bytes: Vec<u8>) -> Captured {
    captured(bytes, CaptureExit::Success)
}
fn value(v: serde_json::Value) -> Captured {
    ok(serde_json::to_vec(&v).unwrap())
}
fn trace(replies: Vec<(Phase, Captured)>) -> Trace {
    Trace {
        replies: replies.into(),
        now: Duration::ZERO,
        calls: vec![],
    }
}
fn cleanup(b: &Binding) -> Vec<(Phase, Captured)> {
    vec![
        (
            Phase::Inspect,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
        (Phase::Inspect, value(inspect(b, false, 0))),
        (
            Phase::Inspect,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
        (Phase::Inspect, value(inspect(b, false, 0))),
        (
            Phase::Remove,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
        (Phase::Inspect, ok(vec![])),
    ]
}
#[test]
fn worker_exit_and_cleanup_are_independent_from_successful_transport_exit() {
    for exit in [0, 7] {
        let root = tempfile::tempdir().unwrap();
        let b = binding(root.path());
        let mut replies = vec![
            (Phase::ImageInspect, value(image(&b))),
            (
                Phase::Create,
                ok(format!("{}\n", cid().value()).into_bytes()),
            ),
            (Phase::Inspect, value(inspect(&b, false, 0))),
            (
                Phase::Start,
                ok(format!("{}\n", cid().value()).into_bytes()),
            ),
            (Phase::Wait, ok(format!("{exit}\n").into_bytes())),
            (Phase::Inspect, value(inspect(&b, false, exit))),
            (Phase::Logs, ok(b"actual trace bytes".to_vec())),
        ];
        replies.extend(cleanup(&b));
        let cancel = Cancellation::default();
        let result = Controller::new(
            trace(replies),
            IntentTrace::default(),
            b,
            |_| true,
            &cancel,
            Duration::from_secs(60),
        )
        .unwrap()
        .run();
        assert_eq!(result.observation.container_exit_code, Some(exit));
        assert_eq!(
            result.observation.status,
            if exit == 0 {
                Status::CompletedNonAuthoritative
            } else {
                Status::Failed
            }
        );
        assert_eq!(result.observation.cleanup, Cleanup::VerifiedAbsent);
        assert!(result.observation.journal_retained);
        assert!(!result.observation.authoritative);
        assert_eq!(result.observation.independent_judge, "UNAVAILABLE");
        assert_eq!(result.observation.transport.len(), 13);
        assert!(result.logs.is_some());
    }
}
#[test]
fn absent_or_invalid_admission_and_failed_intent_cannot_create_a_worker() {
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let cancel = Cancellation::default();
    let result = Controller::new(
        trace(vec![]),
        IntentTrace::default(),
        b,
        |_| false,
        &cancel,
        Duration::from_secs(60),
    )
    .unwrap()
    .run();
    assert!(result.observation.transport.is_empty());
    assert_ne!(result.observation.status, Status::CompletedNonAuthoritative);
    let b = binding(root.path());
    let image = value(image(&b));
    let result = Controller::new(
        trace(vec![(Phase::ImageInspect, image)]),
        IntentTrace {
            calls: 0,
            fail: Some(1),
        },
        b,
        |_| true,
        &cancel,
        Duration::from_secs(60),
    )
    .unwrap()
    .run();
    assert_eq!(result.observation.transport.len(), 1);
    assert_eq!(result.observation.status, Status::Unavailable);
    assert_eq!(result.observation.cleanup, Cleanup::NotNeeded);
}
#[test]
fn cid_journal_failure_still_cleans_exact_observed_container_without_start() {
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let mut replies = vec![
        (Phase::ImageInspect, value(image(&b))),
        (
            Phase::Create,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
    ];
    let mut clean = cleanup(&b);
    clean.insert(4, (Phase::Logs, ok(vec![])));
    replies.extend(clean);
    let cancel = Cancellation::default();
    let result = Controller::new(
        trace(replies),
        IntentTrace {
            calls: 0,
            fail: Some(2),
        },
        b,
        |_| true,
        &cancel,
        Duration::from_secs(60),
    )
    .unwrap()
    .run();
    assert_eq!(result.observation.status, Status::Incomplete);
    assert_eq!(result.observation.cleanup, Cleanup::VerifiedAbsent);
    assert!(!result.observation.journal_retained);
    assert!(
        !result
            .observation
            .transport
            .iter()
            .any(|o| o.phase == Phase::Start)
    );
}
#[test]
fn ambiguous_create_recovers_only_one_matching_container_and_never_starts_it() {
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let mut replies = vec![
        (Phase::ImageInspect, value(image(&b))),
        (Phase::Create, captured(vec![], CaptureExit::Deadline)),
        (
            Phase::Recover,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
        (Phase::Inspect, value(inspect(&b, false, 0))),
    ];
    let mut clean = cleanup(&b);
    clean.insert(4, (Phase::Logs, ok(vec![])));
    replies.extend(clean);
    let cancel = Cancellation::default();
    let result = Controller::new(
        trace(replies),
        IntentTrace::default(),
        b,
        |_| true,
        &cancel,
        Duration::from_secs(60),
    )
    .unwrap()
    .run();
    assert_eq!(result.observation.status, Status::Deadline);
    assert_eq!(result.observation.cleanup, Cleanup::VerifiedAbsent);
    assert_eq!(result.observation.container_exit_code, None);
    assert!(
        !result
            .observation
            .transport
            .iter()
            .any(|o| o.phase == Phase::Start)
    );
}
#[test]
fn cancelled_before_admission_never_calls_backend() {
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let cancel = Cancellation::default();
    cancel.cancel();
    let result = Controller::new(
        trace(vec![]),
        IntentTrace::default(),
        b,
        |_| true,
        &cancel,
        Duration::from_secs(60),
    )
    .unwrap()
    .run();
    assert_eq!(result.observation.status, Status::Cancelled);
    assert!(result.observation.transport.is_empty());
}
#[test]
fn cancellation_after_start_preserves_unknown_worker_exit_and_uses_independent_cleanup() {
    struct Cancelling {
        inner: Trace,
        cancel: Cancellation,
    }
    impl Backend for Cancelling {
        fn now(&self) -> Duration {
            self.inner.now()
        }
        fn call(&mut self, b: &Binding, a: &Action, t: Duration, c: &Cancellation) -> Captured {
            if matches!(a, Action::Wait(_)) {
                self.cancel.cancel();
            }
            if matches!(
                a,
                Action::Query(_) | Action::Stop(_) | Action::Remove(_) | Action::Logs(_)
            ) {
                assert!(!c.cancelled());
            }
            self.inner.call(b, a, t, c)
        }
    }
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let cancel = Cancellation::default();
    let replies = vec![
        (Phase::ImageInspect, value(image(&b))),
        (
            Phase::Create,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
        (Phase::Inspect, value(inspect(&b, false, 0))),
        (
            Phase::Start,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
        (Phase::Wait, captured(vec![], CaptureExit::Cancelled)),
        (
            Phase::Inspect,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
        (Phase::Inspect, value(inspect(&b, true, 0))),
        (Phase::Stop, ok(format!("{}\n", cid().value()).into_bytes())),
        (
            Phase::Inspect,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
        (Phase::Inspect, value(inspect(&b, false, 137))),
        (Phase::Logs, ok(b"cancelled transport log tail".to_vec())),
        (
            Phase::Remove,
            ok(format!("{}\n", cid().value()).into_bytes()),
        ),
        (Phase::Inspect, ok(vec![])),
    ];
    let backend = Cancelling {
        inner: trace(replies),
        cancel: cancel.clone(),
    };
    let result = Controller::new(
        backend,
        IntentTrace::default(),
        b,
        |_| true,
        &cancel,
        Duration::from_secs(60),
    )
    .unwrap()
    .run();
    assert_eq!(result.observation.status, Status::Cancelled);
    assert_eq!(result.observation.container_exit_code, None);
    assert_eq!(result.observation.cleanup, Cleanup::VerifiedAbsent);
    assert!(result.logs.is_some());
}
#[test]
fn multiple_recovery_ids_quarantine_without_unowned_deletion() {
    let root = tempfile::tempdir().unwrap();
    let b = binding(root.path());
    let replies = vec![
        (Phase::ImageInspect, value(image(&b))),
        (
            Phase::Create,
            captured(vec![], CaptureExit::Failed(Some(1))),
        ),
        (
            Phase::Recover,
            ok(format!("{}\n{}\n", cid().value(), "b".repeat(64)).into_bytes()),
        ),
    ];
    let cancel = Cancellation::default();
    let result = Controller::new(
        trace(replies),
        IntentTrace::default(),
        b,
        |_| true,
        &cancel,
        Duration::from_secs(60),
    )
    .unwrap()
    .run();
    assert_eq!(result.observation.status, Status::Quarantined);
    assert!(
        !result
            .observation
            .transport
            .iter()
            .any(|o| matches!(o.phase, Phase::Stop | Phase::Kill | Phase::Remove))
    );
}
