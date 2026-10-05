use super::*;

fn item(case: &ObservedCase, id: AssertionId, step: Option<u64>) -> &Assertion {
    case.assertions
        .iter()
        .find(|item| item.id == id && item.step == step)
        .unwrap()
}

#[test]
fn fixed_case_captures_every_performed_tick_and_closed_predicate() {
    let observed = run();
    assert_eq!(
        observed.calibration,
        Calibration::ValidLocalControlsObserved
    );
    let case = &observed.case;
    assert_eq!(case.outcome, CaseOutcome::LocalCasePass);
    assert_eq!((case.selected, case.started, case.ignored), (1, 1, 0));
    assert_eq!((case.planned_actions, case.executed_actions), (18, 18));
    assert_eq!(
        (
            case.planned_assertions,
            case.asserted,
            case.passed,
            case.failed
        ),
        (178, 178, 178, 0)
    );
    let trace = case.trace.as_ref().unwrap();
    assert_eq!(
        trace.initial.unit.position,
        WorldPosition::new(32_256, 32_256)
    );
    assert_eq!(trace.initial, trace.after_unknown);
    assert_eq!(trace.unknown_result, MoveOutcome::UnknownEntity);
    assert_eq!(trace.move_result, Some(MoveOutcome::Accepted));
    assert_eq!(trace.actions.len(), 18);
    assert!(matches!(
        trace.actions[0],
        Action::MoveOrder {
            entity_id: 999,
            outcome: MoveOutcome::UnknownEntity,
            ..
        }
    ));
    assert!(matches!(
        trace.actions[1],
        Action::MoveOrder {
            entity_id: 0,
            outcome: MoveOutcome::Accepted,
            ..
        }
    ));
    assert_eq!(trace.steps.len(), 16);
    for (index, snapshot) in trace.steps.iter().enumerate() {
        let step = index as u64 + 1;
        assert_eq!(snapshot.tick, step);
        assert!(matches!(trace.actions[index + 2], Action::Advance { tick } if tick == step));
        assert_eq!(
            snapshot.unit.position,
            WorldPosition::new(32_256 + 128 * step as i32, 32_256)
        );
        assert_eq!(
            snapshot.unit.previous_position,
            WorldPosition::new(32_256 + 128 * index as i32, 32_256)
        );
        assert_eq!(snapshot.unit.moving, step < 16);
        assert!(!snapshot.unit.planning);
        assert_eq!(snapshot.order.is_some(), step < 16);
        assert_eq!(snapshot.active_movers, u64::from(step < 16));
        assert_eq!(
            snapshot.target_units,
            if step >= 12 { vec![0] } else { vec![] }
        );
        assert_eq!(
            snapshot.origin_units,
            if step < 4 { vec![0] } else { vec![] }
        );
        assert_eq!(
            case.assertions
                .iter()
                .filter(|item| item.step == Some(step))
                .count(),
            10
        );
    }
    assert_eq!(
        trace.steps[0].unit.position,
        WorldPosition::new(32_384, 32_256)
    );
    let last = trace.steps.last().unwrap();
    assert_eq!(last.unit.position, WorldPosition::new(34_304, 32_256));
    assert_eq!(last.target_units, vec![0]);
    assert!(last.origin_units.is_empty());
    assert!(last.order.is_none());
    assert_eq!(
        item(case, AssertionId::RefusalUnchanged, None).actual,
        Fact::Bool(true)
    );
    assert!(
        case.assertions
            .iter()
            .all(|item| item.outcome == AssertionOutcome::Pass)
    );
}

#[test]
fn production_controls_are_fresh_actions_and_actual_weak_validator_calls() {
    let observed = run();
    let controls = &observed.controls;
    assert_eq!(controls.strong_good, CaseOutcome::LocalCasePass);
    assert_eq!(controls.strong_bad, CaseOutcome::LocalCaseFail);
    assert_eq!(controls.weak_good, WeakOutcome::AcceptedClaims);
    assert_eq!(controls.weak_bad, WeakOutcome::AcceptedClaims);
    let good = observed.case.trace.as_ref().unwrap();
    let bad = controls.bad_journey.trace.as_ref().unwrap();
    assert_eq!(bad.initial, good.initial);
    assert_eq!(bad.after_unknown, good.after_unknown);
    assert_eq!(bad.script, Script::OmittedOrder);
    assert_eq!(bad.move_result, None);
    assert_eq!(bad.executed_actions, 17);
    assert_eq!(bad.steps.len(), 16);
    for snapshot in &bad.steps {
        assert_eq!(snapshot.unit.position, bad.initial.unit.position);
        assert!(!snapshot.unit.moving);
        assert!(!snapshot.unit.planning);
        assert!(snapshot.order.is_none());
    }
    assert_eq!(bad.steps.last().unwrap().tick, 16);
    assert_eq!(
        item(&controls.bad_journey, AssertionId::Position, Some(16)).outcome,
        AssertionOutcome::Fail
    );
    assert_eq!(
        item(&controls.bad_journey, AssertionId::Tick, Some(16)).outcome,
        AssertionOutcome::Pass
    );
    let first = weak_claims();
    let second = weak_claims();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert_eq!(weak(&first), WeakOutcome::AcceptedClaims);
    let mut incomplete = second;
    incomplete.journeys[0].completed = false;
    assert_eq!(weak(&incomplete), WeakOutcome::RejectedClaims);
    // Repeated public calls perform new worlds, not an imported/cached verdict.
    assert_eq!(
        serde_json::to_value(run()).unwrap(),
        serde_json::to_value(observed).unwrap()
    );
}

#[test]
fn finite_actual_action_variants_cannot_pass_the_fixed_oracle() {
    for script in [
        Script::WrongTarget,
        Script::UnderTicks,
        Script::UnknownEntity,
        Script::OmittedOrder,
    ] {
        let case = observe(script);
        assert_eq!(case.outcome, CaseOutcome::LocalCaseFail, "{script:?}");
        assert_eq!(case.asserted, 178);
        assert!(case.failed > 0);
        assert_eq!(
            item(&case, AssertionId::Position, Some(16)).outcome,
            AssertionOutcome::Fail
        );
        let trace = case.trace.as_ref().unwrap();
        match script {
            Script::WrongTarget => {
                assert_eq!(trace.move_result, Some(MoveOutcome::Accepted));
                assert_eq!(trace.executed_actions, 18);
                assert_eq!(
                    trace.steps.last().unwrap().unit.position,
                    WorldPosition::new(33_280, 32_256)
                );
                assert_eq!(
                    item(&case, AssertionId::OrderDestination, None).outcome,
                    AssertionOutcome::Fail
                );
            }
            Script::UnderTicks => {
                assert_eq!(trace.steps.len(), 15);
                assert_eq!(trace.executed_actions, 17);
                assert_eq!(
                    item(&case, AssertionId::Position, Some(16)).actual,
                    Fact::Unavailable
                );
            }
            Script::UnknownEntity => {
                assert_eq!(trace.move_result, Some(MoveOutcome::UnknownEntity));
                assert_eq!(trace.executed_actions, 18);
                assert_eq!(
                    item(&case, AssertionId::MoveAccepted, None).outcome,
                    AssertionOutcome::Fail
                );
            }
            Script::OmittedOrder => assert_eq!(trace.move_result, None),
            Script::Normal => unreachable!("not a negative control"),
        }
    }
}

#[test]
fn missing_controls_or_wrong_counts_are_inconclusive_not_qualification() {
    let accepted = WeakOutcome::AcceptedClaims;
    let good = observe(Script::Normal);
    let mut bad = observe(Script::OmittedOrder);
    assert_eq!(
        calibrated(&good, &bad, accepted, accepted),
        Calibration::ValidLocalControlsObserved
    );
    for (weak_good, weak_bad) in [
        (WeakOutcome::RejectedClaims, accepted),
        (accepted, WeakOutcome::RejectedClaims),
    ] {
        assert_eq!(
            calibrated(&good, &bad, weak_good, weak_bad),
            Calibration::Inconclusive
        );
    }
    // Synthetic evaluator-metadata fault injection is not a performed journey.
    for assertion in &mut bad.assertions {
        if assertion.id == AssertionId::Position && assertion.step == Some(16) {
            assertion.outcome = AssertionOutcome::Pass;
        }
    }
    assert_eq!(
        calibrated(&good, &bad, accepted, accepted),
        Calibration::Inconclusive
    );
    let bad = observe(Script::OmittedOrder);
    for fault in 0..11 {
        let mut good = observe(Script::Normal);
        match fault {
            0 => good.executed_actions = 0,
            1 => good.asserted = 0,
            2 => good.started = 0,
            3 => good.ignored = 1,
            4 => good.selected = 0,
            5 => good.planned_actions = 0,
            6 => good.planned_assertions = 0,
            7 => good.passed = 0,
            8 => good.failed = 1,
            9 => good.assertions.clear(),
            _ => good.outcome = CaseOutcome::NotStarted,
        }
        assert_eq!(
            calibrated(&good, &bad, accepted, accepted),
            Calibration::Inconclusive
        );
    }
}

#[test]
fn synthetic_unavailable_mapping_preserves_actual_count_without_inventing_execution() {
    for reason in [
        SetupFailure::Configuration,
        SetupFailure::World,
        SetupFailure::Spawn,
        SetupFailure::MissingUnit,
    ] {
        for executed_actions in [0, 3] {
            let case = observe_performed(Err(controller::Failure {
                reason,
                executed_actions,
            }));
            assert_eq!(
                case.outcome,
                if executed_actions == 0 {
                    CaseOutcome::NotStarted
                } else {
                    CaseOutcome::ObservationUnavailable
                }
            );
            assert_eq!(case.executed_actions, executed_actions);
            assert_eq!(case.started, u64::from(executed_actions > 0));
            assert_eq!((case.asserted, case.passed, case.failed), (0, 0, 0));
            assert_eq!(case.setup_failure, Some(reason));
            assert!(case.trace.is_none());
            assert!(case.assertions.is_empty());
            let bad = observe(Script::OmittedOrder);
            assert_eq!(
                calibrated(
                    &case,
                    &bad,
                    WeakOutcome::AcceptedClaims,
                    WeakOutcome::AcceptedClaims
                ),
                Calibration::Inconclusive
            );
        }
    }
}

#[test]
fn semantic_output_never_authenticates_source_binary_or_whole_rts() {
    let value = serde_json::to_value(run()).unwrap();
    for (key, expected) in [
        (
            "assessment",
            "LOCAL_SEMANTIC_CALIBRATION_OBSERVED_NON_AUTHORITATIVE",
        ),
        ("qualification", "UNQUALIFIED"),
        ("independent_qa", "NOT_ASSESSED"),
        ("root_cause", "ROOT_CAUSE_NOT_ASSESSED"),
        ("external_judge_capability", "UNAVAILABLE"),
        ("source_identity", "UNAVAILABLE"),
        ("served_build_binding", "UNAVAILABLE"),
        ("process_exit", "NOT_APPLICABLE_IN_PROCESS"),
        ("retention", "CONTROLLER_MEMORY"),
        ("publication", "MCP_RESPONSE_ONLY"),
    ] {
        assert_eq!(value[key], expected);
    }
    assert_eq!(value["authoritative"], false);
    assert_eq!(value["criterion"], "movement-arrival-v1");
    assert!(value.get("status").is_none());
    assert!(value.get("journeys").is_none());
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(bytes.len() < 128 * 1024);
    let text = String::from_utf8(bytes).unwrap();
    for forbidden in [
        "calibration-placeholder.bin",
        "stdout",
        "stderr",
        "resume_token",
        "APPROVED",
    ] {
        assert!(!text.contains(forbidden));
    }
}
