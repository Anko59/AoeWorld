//! Fixed oracle: expected facts do not come from candidate reports or engine outputs.
use super::{
    ACTIONS, ASSERTIONS, Assertion, AssertionId, AssertionOutcome, CaseOutcome, Fact, MoveOutcome,
    ObservedCase, Snapshot, TICKS, Trace,
};
use aoe_core::WorldPosition;

fn assertion(id: AssertionId, step: Option<u64>, expected: Fact, actual: Fact) -> Assertion {
    let outcome = if actual == expected {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail
    };
    Assertion {
        id,
        step,
        expected,
        actual,
        outcome,
    }
}
fn destination(snapshot: &Snapshot) -> Fact {
    snapshot
        .order
        .as_ref()
        .map_or(Fact::Absent, |order| Fact::Position(order.destination))
}
pub(super) fn evaluate(trace: Trace) -> ObservedCase {
    use AssertionId as Id;
    use Fact::{Bool, Move, Number, Position};
    let origin = WorldPosition::new(32_256, 32_256);
    let target = WorldPosition::new(34_304, 32_256);
    let initial = &trace.initial;
    let ordered = &trace.after_order;
    let mut assertions = vec![
        assertion(Id::InitialTick, None, Number(0), Number(initial.tick)),
        assertion(
            Id::InitialPosition,
            None,
            Position(origin),
            Position(initial.unit.position),
        ),
        assertion(
            Id::InitialEntity,
            None,
            Number(0),
            Number(u64::from(initial.unit.id)),
        ),
        assertion(
            Id::InitialPlayer,
            None,
            Number(0),
            Number(u64::from(initial.unit.player)),
        ),
        assertion(
            Id::InitialMoving,
            None,
            Bool(false),
            Bool(initial.unit.moving),
        ),
        assertion(
            Id::InitialPlanning,
            None,
            Bool(false),
            Bool(initial.unit.planning),
        ),
        assertion(Id::InitialOrder, None, Fact::Absent, destination(initial)),
        assertion(
            Id::InitialActiveMovers,
            None,
            Number(0),
            Number(initial.active_movers),
        ),
        assertion(
            Id::UnknownRejected,
            None,
            Move(MoveOutcome::UnknownEntity),
            Move(trace.unknown_result),
        ),
        assertion(
            Id::RefusalUnchanged,
            None,
            Bool(true),
            Bool(initial == &trace.after_unknown),
        ),
        assertion(
            Id::MoveAccepted,
            None,
            Move(MoveOutcome::Accepted),
            trace.move_result.map_or(Fact::Absent, Move),
        ),
        assertion(
            Id::OrderPosition,
            None,
            Position(origin),
            Position(ordered.unit.position),
        ),
        assertion(Id::OrderMoving, None, Bool(true), Bool(ordered.unit.moving)),
        assertion(
            Id::OrderPlanning,
            None,
            Bool(false),
            Bool(ordered.unit.planning),
        ),
        assertion(
            Id::OrderDestination,
            None,
            Position(target),
            destination(ordered),
        ),
        assertion(
            Id::OrderActiveMovers,
            None,
            Number(1),
            Number(ordered.active_movers),
        ),
        assertion(
            Id::ExecutedActions,
            None,
            Number(ACTIONS),
            Number(trace.executed_actions),
        ),
        assertion(
            Id::StepCount,
            None,
            Number(TICKS as u64),
            Number(trace.steps.len() as u64),
        ),
    ];
    for index in 0..TICKS {
        let step = index as u64 + 1;
        let x = 32_256 + 128 * (index as i32 + 1);
        let position = WorldPosition::new(x, 32_256);
        let previous = WorldPosition::new(x - 128, 32_256);
        let active = index + 1 < TICKS;
        let expected = [
            (Id::Tick, Number(step)),
            (Id::Position, Position(position)),
            (Id::PreviousPosition, Position(previous)),
            (Id::Moving, Bool(active)),
            (Id::Planning, Bool(false)),
            (Id::OrderPresent, Bool(active)),
            (
                Id::Destination,
                if active {
                    Position(target)
                } else {
                    Fact::Absent
                },
            ),
            (Id::ActiveMovers, Number(u64::from(active))),
            (
                Id::TargetQuery,
                Fact::Entities(if x >= 33_792 { vec![0] } else { vec![] }),
            ),
            (
                Id::OriginQuery,
                Fact::Entities(if x < 32_768 { vec![0] } else { vec![] }),
            ),
        ];
        for (id, expected) in expected {
            let actual = trace
                .steps
                .get(index)
                .map_or(Fact::Unavailable, |snapshot| match id {
                    Id::Tick => Number(snapshot.tick),
                    Id::Position => Position(snapshot.unit.position),
                    Id::PreviousPosition => Position(snapshot.unit.previous_position),
                    Id::Moving => Bool(snapshot.unit.moving),
                    Id::Planning => Bool(snapshot.unit.planning),
                    Id::OrderPresent => Bool(snapshot.order.is_some()),
                    Id::Destination => destination(snapshot),
                    Id::ActiveMovers => Number(snapshot.active_movers),
                    Id::TargetQuery => Fact::Entities(snapshot.target_units.clone()),
                    Id::OriginQuery => Fact::Entities(snapshot.origin_units.clone()),
                    _ => Fact::Unavailable,
                });
            assertions.push(assertion(id, Some(step), expected, actual));
        }
    }
    let passed = assertions
        .iter()
        .filter(|item| item.outcome == AssertionOutcome::Pass)
        .count();
    let failed = assertions.len() - passed;
    ObservedCase {
        outcome: if assertions.len() == ASSERTIONS && failed == 0 {
            CaseOutcome::LocalCasePass
        } else {
            CaseOutcome::LocalCaseFail
        },
        selected: 1,
        started: 1,
        ignored: 0,
        planned_actions: ACTIONS,
        executed_actions: trace.executed_actions,
        planned_assertions: ASSERTIONS,
        asserted: assertions.len(),
        passed,
        failed,
        setup_failure: None,
        trace: Some(trace),
        assertions,
    }
}
