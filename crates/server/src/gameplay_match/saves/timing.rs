//! Where the time of a restore goes. The simulation reports each phase as it
//! finishes; the clock is here, and runs only for a restore that shows its time.
use aoe_simulation::{GameWorld, RestorePhase, RestoreReport, SnapshotError, WorldSnapshot};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use tracing::field::{DebugValue, debug};

/// Puts a saved world back, as `GameWorld::restore_snapshot` does.
pub(super) type Restore<R> =
    fn(&mut GameWorld, &WorldSnapshot, [u8; 32], &dyn Fn() -> bool) -> Result<R, SnapshotError>;

/// Microseconds by phase, printed as a map.
type Phases = DebugValue<BTreeMap<&'static str, u64>>;

/// The microseconds of a restore, whole and by phase, as log fields: a match
/// that restored nothing has neither.
pub(super) type Spent = (Option<u64>, Option<Phases>);

/// `GameWorld::restore_snapshot` and the time it took. The rest is what the
/// other phases leave of the whole.
pub(super) fn restore(
    world: &mut GameWorld,
    saved: &WorldSnapshot,
    content_hash: [u8; 32],
    cancelled: &dyn Fn() -> bool,
) -> Result<(RestoreReport, Spent), SnapshotError> {
    let started = Instant::now();
    let mut last = started;
    let mut phases = BTreeMap::<&'static str, Duration>::new();
    let mut probe = |phase| {
        let now = Instant::now();
        *phases.entry(name(phase)).or_default() += now.duration_since(last);
        last = now;
    };
    let report = world.restore_snapshot_with_probe(saved, content_hash, cancelled, &mut probe)?;
    let whole = started.elapsed();
    phases.remove(name(RestorePhase::Rest));
    let others: Duration = phases.values().sum();
    phases.insert(name(RestorePhase::Rest), whole.saturating_sub(others));
    let phases: BTreeMap<_, _> = phases
        .into_iter()
        .map(|(name, spent)| (name, micros(spent)))
        .collect();
    Ok((report, (Some(micros(whole)), Some(debug(phases)))))
}

fn micros(spent: Duration) -> u64 {
    spent.as_micros() as u64
}

fn name(phase: RestorePhase) -> &'static str {
    match phase {
        RestorePhase::Resources => "resources",
        RestorePhase::Explored => "explored",
        RestorePhase::Buildings => "buildings",
        RestorePhase::Units => "units",
        RestorePhase::FogAndVision => "fog_and_vision",
        RestorePhase::JobsAndOrders => "jobs_and_orders",
        RestorePhase::Rest => "rest",
    }
}
