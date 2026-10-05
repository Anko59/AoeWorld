//! Locally performed engine predicates, not independent judging or exploratory QA.
use super::{Journey, REQUIRED, Report, Status, validate};
use aoe_core::{TileCoord, WorldPosition};
use serde::Serialize;
mod controller;
mod evaluator;
use controller::{Script, SetupFailure};
use evaluator::evaluate;

const ACTIONS: u64 = 18;
const TICKS: usize = 16;
const ASSERTIONS: usize = 18 + TICKS * 10;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum MoveOutcome {
    Accepted,
    NoMovement,
    UnknownEntity,
    OtherRejection,
}
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
struct Unit {
    id: u32,
    player: u16,
    position: WorldPosition,
    previous_position: WorldPosition,
    moving: bool,
    planning: bool,
    facing: u8,
}
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
struct Order {
    origin: WorldPosition,
    destination: WorldPosition,
    waypoint: WorldPosition,
    target_tile: TileCoord,
    segment_length: u32,
    travelled: u32,
    speed_carry: u64,
}
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
struct Snapshot {
    tick: u64,
    unit: Unit,
    order: Option<Order>,
    active_movers: u64,
    target_units: Vec<u32>,
    origin_units: Vec<u32>,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
enum Action {
    MoveOrder {
        entity_id: u32,
        destination: WorldPosition,
        outcome: MoveOutcome,
    },
    Advance {
        tick: u64,
    },
}
#[derive(Debug, Serialize)]
struct Trace {
    script: Script,
    actions: Vec<Action>,
    initial: Snapshot,
    unknown_result: MoveOutcome,
    after_unknown: Snapshot,
    move_result: Option<MoveOutcome>,
    after_order: Snapshot,
    steps: Vec<Snapshot>,
    executed_actions: u64,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum AssertionId {
    InitialTick,
    InitialPosition,
    InitialEntity,
    InitialPlayer,
    InitialMoving,
    InitialPlanning,
    InitialOrder,
    InitialActiveMovers,
    UnknownRejected,
    RefusalUnchanged,
    MoveAccepted,
    OrderPosition,
    OrderMoving,
    OrderPlanning,
    OrderDestination,
    OrderActiveMovers,
    ExecutedActions,
    StepCount,
    Tick,
    Position,
    PreviousPosition,
    Moving,
    Planning,
    OrderPresent,
    Destination,
    ActiveMovers,
    TargetQuery,
    OriginQuery,
}
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
enum Fact {
    Number(u64),
    Bool(bool),
    Position(WorldPosition),
    Move(MoveOutcome),
    Entities(Vec<u32>),
    Absent,
    Unavailable,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum AssertionOutcome {
    Pass,
    Fail,
}
#[derive(Debug, Serialize)]
struct Assertion {
    id: AssertionId,
    step: Option<u64>,
    expected: Fact,
    actual: Fact,
    outcome: AssertionOutcome,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum CaseOutcome {
    LocalCasePass,
    LocalCaseFail,
    NotStarted,
    ObservationUnavailable,
}
#[derive(Debug, Serialize)]
struct ObservedCase {
    outcome: CaseOutcome,
    selected: u64,
    started: u64,
    ignored: u64,
    planned_actions: u64,
    executed_actions: u64,
    planned_assertions: usize,
    asserted: usize,
    passed: usize,
    failed: usize,
    setup_failure: Option<SetupFailure>,
    trace: Option<Trace>,
    assertions: Vec<Assertion>,
}

fn observe(script: Script) -> ObservedCase {
    observe_performed(controller::perform(script))
}
fn observe_performed(performed: Result<Trace, controller::Failure>) -> ObservedCase {
    match performed {
        Ok(trace) => evaluate(trace),
        Err(failure) => ObservedCase {
            outcome: if failure.executed_actions == 0 {
                CaseOutcome::NotStarted
            } else {
                CaseOutcome::ObservationUnavailable
            },
            selected: 1,
            started: u64::from(failure.executed_actions > 0),
            ignored: 0,
            planned_actions: ACTIONS,
            executed_actions: failure.executed_actions,
            planned_assertions: ASSERTIONS,
            asserted: 0,
            passed: 0,
            failed: 0,
            setup_failure: Some(failure.reason),
            trace: None,
            assertions: vec![],
        },
    }
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum WeakOutcome {
    AcceptedClaims,
    RejectedClaims,
}
fn weak_claims() -> Report {
    Report {
        version: 1,
        budget: "fast".into(),
        build: "local-calibration-claim-not-build-identity".into(),
        scenario: "local-calibration-claim-not-six-journeys".into(),
        status: Status::Pass,
        journeys: REQUIRED
            .iter()
            .map(|name| Journey {
                name: (*name).into(),
                completed: true,
                evidence: vec!["calibration-placeholder.bin".into()],
            })
            .collect(),
        findings: vec![],
    }
}
fn weak(report: &Report) -> WeakOutcome {
    if validate(report).is_ok() {
        WeakOutcome::AcceptedClaims
    } else {
        WeakOutcome::RejectedClaims
    }
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Calibration {
    ValidLocalControlsObserved,
    Inconclusive,
}
#[derive(Debug, Serialize)]
struct Controls {
    strong_good: CaseOutcome,
    strong_bad: CaseOutcome,
    weak_good: WeakOutcome,
    weak_bad: WeakOutcome,
    bad_journey: ObservedCase,
}
fn calibrated(
    good: &ObservedCase,
    bad: &ObservedCase,
    weak_good: WeakOutcome,
    weak_bad: WeakOutcome,
) -> Calibration {
    let arrival_rejected = bad.assertions.iter().any(|item| {
        item.id == AssertionId::Position
            && item.step == Some(16)
            && item.outcome == AssertionOutcome::Fail
    });
    if good.outcome == CaseOutcome::LocalCasePass
        && bad.outcome == CaseOutcome::LocalCaseFail
        && good.selected == 1
        && bad.selected == 1
        && good.planned_actions == ACTIONS
        && bad.planned_actions == ACTIONS
        && good.executed_actions == ACTIONS
        && bad.executed_actions == ACTIONS - 1
        && good.planned_assertions == ASSERTIONS
        && bad.planned_assertions == ASSERTIONS
        && good.asserted == ASSERTIONS
        && bad.asserted == ASSERTIONS
        && good.passed == ASSERTIONS
        && good.failed == 0
        && bad.failed > 0
        && bad.passed + bad.failed == ASSERTIONS
        && good.assertions.len() == ASSERTIONS
        && bad.assertions.len() == ASSERTIONS
        && good.started == 1
        && bad.started == 1
        && good.ignored == 0
        && bad.ignored == 0
        && arrival_rejected
        && weak_good == WeakOutcome::AcceptedClaims
        && weak_bad == WeakOutcome::AcceptedClaims
    {
        Calibration::ValidLocalControlsObserved
    } else {
        Calibration::Inconclusive
    }
}
#[derive(Debug, Serialize)]
enum RootCause {
    #[serde(rename = "ROOT_CAUSE_NOT_ASSESSED")]
    NotAssessed,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ExternalJudgeCapability {
    Unavailable,
}
#[derive(Debug, Serialize)]
pub(crate) struct Observation {
    schema: u16,
    criterion: &'static str,
    assessment: &'static str,
    authoritative: bool,
    qualification: &'static str,
    independent_qa: &'static str,
    root_cause: RootCause,
    external_judge_capability: ExternalJudgeCapability,
    source_identity: &'static str,
    served_build_binding: &'static str,
    process_exit: &'static str,
    retention: &'static str,
    publication: &'static str,
    calibration: Calibration,
    case: ObservedCase,
    controls: Controls,
}
pub(crate) fn run() -> Observation {
    let case = observe(Script::Normal);
    let bad_journey = observe(Script::OmittedOrder);
    // The same claims are checked twice; neither report is an observation input.
    // This intentionally weak control calls the actual legacy structural validator.
    let report = weak_claims();
    let weak_good = weak(&report);
    let weak_bad = weak(&report);
    let calibration = calibrated(&case, &bad_journey, weak_good, weak_bad);
    let controls = Controls {
        strong_good: case.outcome,
        strong_bad: bad_journey.outcome,
        weak_good,
        weak_bad,
        bad_journey,
    };
    Observation {
        schema: 1,
        criterion: "movement-arrival-v1",
        assessment: "LOCAL_SEMANTIC_CALIBRATION_OBSERVED_NON_AUTHORITATIVE",
        authoritative: false,
        qualification: "UNQUALIFIED",
        independent_qa: "NOT_ASSESSED",
        root_cause: RootCause::NotAssessed,
        external_judge_capability: ExternalJudgeCapability::Unavailable,
        source_identity: "UNAVAILABLE",
        served_build_binding: "UNAVAILABLE",
        process_exit: "NOT_APPLICABLE_IN_PROCESS",
        retention: "CONTROLLER_MEMORY",
        publication: "MCP_RESPONSE_ONLY",
        calibration,
        case,
        controls,
    }
}
#[cfg(test)]
mod tests;
