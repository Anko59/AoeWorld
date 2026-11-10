//! Counters of the per-tick passes that touch every unit, building, animal or
//! journey (movement rework plan P1, `docs/movement/caps.md` section 3).
//!
//! Diagnostics only: the counters are never hashed, never snapshotted and never
//! read by behaviour. They are cumulative; the delta between two reads is the
//! work done in between. Event-driven passes still count the entities they process.
use crate::GameWorld;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// A pass over a whole population, in the order of `PASS_NAMES`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Pass {
    /// `collision_opened`, on each tree felled or building removed: visits
    /// only its subscribers (`resources.rs`), never every unit.
    CollisionOpened,
    /// `collision_closed`, on each building placed or gate closed: visits
    /// only its subscribers (`resources.rs`), never every unit.
    CollisionClosed,
    /// The per-tick `remember_visible_buildings`: every watched building
    /// cloned. Replaced by changed-building events (`visibility/memory.rs`);
    /// never recorded, so it reports 0.
    #[allow(dead_code)]
    RememberVisibleBuildings,
    /// `advance_gates`: every building, every tick. Replaced by the gate
    /// index (`Economy::gates`); never recorded, so it reports 0.
    #[allow(dead_code)]
    AdvanceGates,
    /// The wildlife id collection: every animal, every tick.
    WildlifeIds,
    /// `advance_transports`: every transport journey, every tick.
    AdvanceTransports,
    /// `collect_changed`: changed ids processed each tick, including repeated
    /// reports of an id from different phases before dirty-bit deduplication.
    CollectChanged,
    /// `Tables::refresh`: the dense per-tick lookups of every entity id.
    TablesRefresh,
}

pub const PASS_NAMES: [&str; 8] = [
    "collision_opened",
    "collision_closed",
    "remember_visible_buildings",
    "advance_gates",
    "wildlife_ids",
    "advance_transports",
    "collect_changed",
    "tables_refresh",
];

/// What one pass has done since the world was made.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PassCount {
    /// Times the pass ran.
    pub calls: u64,
    /// Entities it went over, summed over its calls.
    pub visited: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Passes {
    calls: [AtomicU64; PASS_NAMES.len()],
    visited: [AtomicU64; PASS_NAMES.len()],
}

impl Passes {
    pub(crate) fn record(&self, pass: Pass, visited: usize) {
        self.calls[pass as usize].fetch_add(1, Relaxed);
        self.visited[pass as usize].fetch_add(visited as u64, Relaxed);
    }
}

impl GameWorld {
    /// Cumulative counters of the whole-population passes, with their names
    /// (`PASS_NAMES`). Diagnostics only.
    pub fn pass_counts(&self) -> Vec<(&'static str, PassCount)> {
        let passes = &self.passes;
        PASS_NAMES
            .iter()
            .enumerate()
            .map(|(at, name)| {
                let count = PassCount {
                    calls: passes.calls[at].load(Relaxed),
                    visited: passes.visited[at].load(Relaxed),
                };
                (*name, count)
            })
            .collect()
    }
}
