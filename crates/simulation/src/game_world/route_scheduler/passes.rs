//! Counters of the per-tick passes that touch every unit, building, animal or
//! journey (movement rework plan P1, `docs/movement/caps.md` section 3).
//!
//! Diagnostics only: the counters are never hashed, never snapshotted and never
//! read by behaviour. They are cumulative; the delta between two reads is the
//! work done in between. Phase 1 takes each pass's `visited` to 0 per tick.
use crate::GameWorld;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// A pass over a whole population, in the order of `PASS_NAMES`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Pass {
    /// `collision_opened`: every unit, on each tree felled or building removed.
    CollisionOpened,
    /// `collision_closed`: every unit, on each building placed or gate closed.
    CollisionClosed,
    /// The per-tick `remember_visible_buildings`: every watched building cloned.
    RememberVisibleBuildings,
    /// `advance_gates`: every building, every tick.
    AdvanceGates,
    /// The wildlife id collection: every animal, every tick.
    WildlifeIds,
    /// `advance_transports`: every transport journey, every tick.
    AdvanceTransports,
    /// `collect_changed`: every changed id sorted and fetched, every tick.
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
