//! A restore reports its parts as a tick does: a callback as each finishes, so
//! a caller can attribute the time without a clock in the simulation.
use super::*;
use crate::RestorePhase as Phase;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Counts route planning, which only the resumed jobs and orders do here.
#[derive(Debug, Default)]
struct Planning(AtomicUsize);

impl crate::PlanningProbe for Planning {
    fn begin(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn end(&self) {}
}

/// A village and a soldier on a long march.
fn saved_march() -> WorldSnapshot {
    let (mut saved, start) = flat();
    saved.initialize_village(PlayerId(0), start).unwrap();
    let at = |dx| {
        aoe_core::WorldPosition::from_tile_center(TileCoord::new(start.x + dx, start.y + 6))
            .unwrap()
    };
    let soldier = saved
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, at(6))
        .unwrap();
    saved.issue_move(soldier, at(40)).unwrap();
    for _ in 0..20 {
        saved.advance();
    }
    saved.snapshot([0; 32]).unwrap()
}

#[test]
fn a_restore_reports_every_phase_as_it_finishes_and_changes_nothing() {
    let snapshot = saved_march();
    let (mut plain, _) = flat();
    let expected = plain
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    assert!(expected.jobs_resumed > 0, "{expected:?}");

    let (mut probed, _) = flat();
    let planning = Arc::new(Planning::default());
    probed.set_planning_probe(planning.clone());
    let mut phases = Vec::new();
    let report = probed
        .restore_snapshot_with_probe(&snapshot, [0; 32], &|| false, &mut |phase: Phase| {
            phases.push((phase, planning.0.load(Ordering::Relaxed)));
        })
        .unwrap();
    assert_eq!(report, expected);
    assert_eq!(probed.canonical_hash(), plain.canonical_hash());

    let first = |wanted: Phase| {
        phases
            .iter()
            .position(|&(phase, _)| phase == wanted)
            .unwrap_or_else(|| panic!("{wanted:?} is never reported: {phases:?}"))
    };
    let at = [
        Phase::Resources,
        Phase::Explored,
        Phase::Buildings,
        Phase::Units,
        Phase::FogAndVision,
        Phase::JobsAndOrders,
        Phase::Rest,
    ]
    .map(first);
    assert!(at[..4].is_sorted(), "{phases:?}");
    assert_eq!(
        phases.last().map(|&(phase, _)| phase),
        Some(Phase::Rest),
        "the end of a restore belongs to a phase"
    );
    // Each phase is reported after its own work: the route planning of the
    // resumed jobs lands in theirs and in no other.
    let mut before = 0;
    for &(phase, planned) in &phases {
        if phase != Phase::JobsAndOrders {
            assert_eq!(planned, before, "{phase:?} holds planning: {phases:?}");
        }
        before = planned;
    }
    assert!(before > 0, "the march is planned again: {phases:?}");
}
