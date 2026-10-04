use super::*;

fn run(mode: &'static str) -> (Receipt, ModelDaemon) {
    let mut controller = ModelDaemon::controller(mode);
    let receipt = controller.run(15000, 15000, false, false);
    (receipt, controller.backend)
}
fn rejected(mode: &'static str, expected: &str) {
    let (receipt, daemon) = run(mode);
    assert_eq!(receipt.status, expected, "{mode}");
    assert!(!receipt.authoritative);
    assert!(!daemon.calls.contains(&"stop-exact-id"));
    assert!(!daemon.calls.contains(&"kill-exact-id"));
    assert!(!daemon.calls.contains(&"remove-exact-id"));
}
#[test]
fn cid_receipts_are_exact_full_lowercase_not_names_or_selectors() {
    assert!(ContainerId::observed("a".repeat(64).as_bytes()).is_ok());
    assert!(ContainerId::observed(format!("{}\n", "a".repeat(64)).as_bytes()).is_ok());
    for bytes in [
        b"name".to_vec(),
        "A".repeat(64).into_bytes(),
        "g".repeat(64).into_bytes(),
        "a".repeat(63).into_bytes(),
        "a".repeat(65).into_bytes(),
        vec![255],
        format!("{}\n{}", "a".repeat(64), "b".repeat(64)).into_bytes(),
        format!(" {}", "a".repeat(64)).into_bytes(),
    ] {
        assert!(ContainerId::observed(&bytes).is_err());
    }
}
#[test]
fn intent_precedes_create_and_removal_requires_final_observed_absence() {
    let (receipt, daemon) = run("success");
    assert_eq!(receipt.status, "COMPLETED_MODEL");
    assert!(!receipt.authoritative);
    assert!(!daemon.object);
    assert_eq!(
        daemon.calls,
        [
            "persist-intent",
            "create",
            "inspect-exact-id",
            "stop-exact-id",
            "inspect-exact-id",
            "remove-exact-id",
            "inspect-exact-id"
        ]
    );
}
#[test]
fn failed_intent_never_creates() {
    rejected("persist-failed", "INCOMPLETE");
}
#[test]
fn short_create_cid_never_mutates() {
    rejected("short-cid", "QUARANTINED");
}
#[test]
fn multiline_create_cid_never_mutates() {
    rejected("multiple-cid", "QUARANTINED");
}
#[test]
fn invalid_utf8_create_cid_never_mutates() {
    rejected("invalid-cid", "QUARANTINED");
}
#[test]
fn truncated_create_receipt_never_mutates_or_adopts_partial_id() {
    rejected("create-truncated", "INCOMPLETE");
}
#[test]
fn substituted_cid_never_mutates() {
    rejected("wrong-cid", "QUARANTINED");
}
#[test]
fn different_service_never_mutates() {
    rejected("wrong-service", "QUARANTINED");
}
#[test]
fn stale_epoch_never_mutates() {
    rejected("wrong-epoch", "QUARANTINED");
}
#[test]
fn different_nonce_never_mutates() {
    rejected("wrong-nonce", "QUARANTINED");
}
#[test]
fn substituted_daemon_never_mutates() {
    rejected("wrong-daemon", "QUARANTINED");
}
#[test]
fn substituted_image_configuration_never_mutates() {
    rejected("wrong-image", "QUARANTINED");
}
#[test]
fn different_security_template_never_mutates() {
    rejected("wrong-security", "QUARANTINED");
}
#[test]
fn absent_identity_fields_never_mutate() {
    rejected("missing-metadata", "INCOMPLETE");
}
#[test]
fn unavailable_daemon_never_mutates() {
    rejected("daemon-lost", "INCOMPLETE");
}
#[test]
fn invalid_utf8_inspection_never_mutates() {
    rejected("invalid-inspect", "INCOMPLETE");
}
#[test]
fn truncated_inspection_never_mutates() {
    rejected("inspect-truncated", "INCOMPLETE");
}
#[test]
fn late_success_is_not_success_and_does_not_spawn_create() {
    rejected("late", "INCOMPLETE");
}
#[test]
fn recovery_adopts_only_one_full_observed_matching_identity() {
    let (receipt, daemon) = run("recover-one");
    assert_eq!(receipt.status, "COMPLETED_MODEL");
    assert!(!daemon.object);
    assert_eq!(daemon.calls[2], "recover-owned-labels");
    assert_eq!(daemon.calls[3], "inspect-exact-id");
}
#[test]
fn recovery_absence_records_no_mutations() {
    let (receipt, daemon) = run("recover-zero");
    assert_eq!(receipt.status, "COMPLETED_MODEL");
    assert_eq!(
        daemon.calls,
        ["persist-intent", "create", "recover-owned-labels"]
    );
}
#[test]
fn ambiguous_recovery_quarantines_without_deletion() {
    rejected("recover-many", "QUARANTINED");
}
#[test]
fn recovery_count_limit_does_not_silently_drop_other_ids() {
    rejected("recover-oversize", "QUARANTINED");
}
#[test]
fn failed_stop_reinspects_before_kill_then_reinspects_before_remove() {
    let (receipt, daemon) = run("stop-failed");
    assert_eq!(receipt.status, "COMPLETED_MODEL");
    assert!(!daemon.object);
    assert_eq!(
        &daemon.calls[3..7],
        [
            "stop-exact-id",
            "inspect-exact-id",
            "kill-exact-id",
            "inspect-exact-id"
        ]
    );
}
#[test]
fn failed_kill_keeps_cleanup_debt() {
    let (receipt, daemon) = run("kill-failed");
    assert_eq!(receipt.status, "INCOMPLETE");
    assert!(daemon.object);
    assert!(!daemon.calls.contains(&"remove-exact-id"));
}
#[test]
fn remove_failure_and_false_zero_exit_do_not_establish_absence() {
    for mode in ["remove-failed", "remove-lies"] {
        let (receipt, daemon) = run(mode);
        assert_eq!(receipt.status, "INCOMPLETE");
        assert!(daemon.object);
    }
}
#[test]
fn changed_metadata_after_stop_prevents_removal() {
    let (receipt, daemon) = run("changed-before-remove");
    assert_eq!(receipt.status, "QUARANTINED");
    assert!(daemon.object);
    assert!(daemon.calls.contains(&"stop-exact-id"));
    assert!(!daemon.calls.contains(&"remove-exact-id"));
}
#[test]
fn zero_admission_and_precancel_prevent_all_spawning() {
    for (reserve, cancelled) in [(0, false), (15000, true)] {
        let mut controller = ModelDaemon::controller("success");
        assert_eq!(
            controller.run(reserve, 15000, cancelled, false).status,
            "INCOMPLETE"
        );
        assert!(controller.backend.calls.is_empty());
    }
}
#[test]
fn zero_cleanup_preserves_object_as_visible_debt() {
    let mut controller = ModelDaemon::controller("success");
    assert_eq!(controller.run(15000, 0, false, false).status, "INCOMPLETE");
    assert!(controller.backend.object);
    assert_eq!(controller.backend.calls, ["persist-intent", "create"]);
}
#[test]
fn workload_cancellation_does_not_cancel_cleanup_reserve() {
    let mut controller = ModelDaemon::controller("success");
    let receipt = controller.run(15000, 15000, false, true);
    assert_eq!(receipt.status, "COMPLETED_MODEL");
    assert!(receipt.workload_cancelled);
    assert!(!controller.backend.object);
    assert!(!receipt.authoritative);
}
#[test]
fn killing_only_client_leaves_daemon_object_until_explicit_owned_cleanup() {
    let mut controller = ModelDaemon::controller("success");
    let cid = controller.acquire(15000, false).unwrap().unwrap();
    // Dropping/restarting the client controller changes no daemon state.
    let daemon = controller.backend;
    assert!(daemon.object);
    let mut restarted = Controller {
        backend: daemon,
        expected: ExpectedIdentity::simulation(),
        events: Vec::new(),
        last_ms: None,
    };
    restarted.cleanup(&cid, 15000).unwrap();
    assert!(!restarted.backend.object);
}
#[test]
fn clock_rollback_between_actions_cannot_extend_admission_or_cleanup() {
    let mut controller = ModelDaemon::controller("success");
    controller
        .call(Action::PersistIntent(controller.expected.clone()), 15000)
        .unwrap();
    let calls = controller.backend.calls.len();
    controller.backend.time = 0;
    assert!(matches!(
        controller.call(Action::Create(controller.expected.clone()), 15000),
        Err(Fault::Incomplete)
    ));
    assert_eq!(controller.backend.calls.len(), calls);
    assert!(!controller.backend.object);

    let mut controller = ModelDaemon::controller("success");
    let cid = controller.acquire(15000, false).unwrap().unwrap();
    let calls = controller.backend.calls.len();
    controller.backend.time = 0;
    assert_eq!(controller.cleanup(&cid, 15000), Err(Fault::Incomplete));
    assert_eq!(controller.backend.calls.len(), calls);
    assert!(controller.backend.object);
}
#[test]
fn admission_deadline_overflow_does_not_start_an_intent() {
    let mut controller = ModelDaemon::controller("success");
    controller.backend.time = u64::MAX - 1;
    assert_eq!(controller.run(15, 15, false, false).status, "INCOMPLETE");
    assert!(controller.backend.calls.is_empty());
}
struct FaultBackend {
    daemon: ModelDaemon,
    mode: &'static str,
    target: &'static str,
}
impl Backend for FaultBackend {
    fn now_ms(&self) -> u64 {
        self.daemon.now_ms()
    }
    fn call(&mut self, action: &Action, timeout_ms: u64) -> Reply {
        let mut reply = self.daemon.call(action, timeout_ms);
        if action.name() == self.target {
            match self.mode {
                "oversize" => reply.bytes = vec![b'x'; 4097],
                "partial" => reply.bytes = vec![255],
                "late" => self.daemon.time += timeout_ms,
                "rollback" => self.daemon.time = 0,
                "truncated" => reply.truncated = true,
                _ => unreachable!(),
            }
        }
        reply
    }
}
#[test]
fn bounded_receipts_reject_oversize_invalid_ack_and_late_mutation_success() {
    for (mode, target) in [
        ("oversize", "inspect-exact-id"),
        ("partial", "stop-exact-id"),
        ("late", "remove-exact-id"),
        ("truncated", "stop-exact-id"),
        ("rollback", "inspect-exact-id"),
    ] {
        let mut controller = Controller {
            backend: FaultBackend {
                daemon: ModelDaemon::new("success"),
                mode,
                target,
            },
            expected: ExpectedIdentity::simulation(),
            events: Vec::new(),
            last_ms: None,
        };
        assert_eq!(
            controller.run(15000, 15000, false, false).status,
            "INCOMPLETE",
            "{mode}"
        );
    }
}
#[test]
fn shared_cleanup_budget_expires_before_new_command_and_caps_reserve() {
    let mut controller = ModelDaemon::controller("success");
    let cid = controller.acquire(15000, false).unwrap().unwrap();
    assert_eq!(controller.cleanup(&cid, 2), Err(Fault::Incomplete));
    assert_eq!(controller.backend.calls.last(), Some(&"stop-exact-id"));
    assert!(controller.backend.object);
    let mut controller = ModelDaemon::controller("success");
    let cid = controller.acquire(15000, false).unwrap().unwrap();
    controller.cleanup(&cid, u64::MAX).unwrap();
    assert!(!controller.backend.object);
}
#[test]
fn confirmed_absence_is_idempotent_and_does_not_mutate() {
    let mut controller = ModelDaemon::controller("success");
    let cid = controller.acquire(15000, false).unwrap().unwrap();
    controller.cleanup(&cid, 15000).unwrap();
    let count = controller.backend.calls.len();
    controller.cleanup(&cid, 15000).unwrap();
    assert_eq!(&controller.backend.calls[count..], ["inspect-exact-id"]);
}
#[test]
fn preview_is_fixed_bounded_false_and_never_gate_pass() {
    let preview = preview_models();
    assert_eq!(preview["authoritative"], false);
    assert_eq!(preview["status"], "MODEL_ONLY");
    let scenarios = preview["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 24);
    for row in scenarios {
        assert_eq!(row["receipt"]["authoritative"], false);
        assert_ne!(row["receipt"]["status"], "PASS");
        assert!(row["receipt"]["events"].as_array().unwrap().len() <= 16);
    }
}
