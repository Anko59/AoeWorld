//! Fixed storage per tick: profiling must not allocate a tree for every sample.
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

pub(super) const NAMES: [&str; 48] = [
    "sessions",
    "world_lock",
    "orders",
    "movement",
    "spacing",
    "wildlife",
    "combat",
    "construction",
    "gathering",
    "research_training",
    "steward_economy",
    "building_memory",
    "changed",
    "resource_journal",
    "fanout_wait",
    "viewers",
    "frame",
    "session_views",
    "delivery",
    "resources",
    "observation_lock",
    "terrain",
    "walk_cache",
    "portals",
    "portal_costs",
    "components",
    "hierarchy",
    "pocket",
    "route_cache",
    "terrain_point",
    "prefetch",
    "search",
    "plan_serve",
    "plan_fast",
    "plan_fair",
    "plan_ahead",
    "propose",
    "apply_loop",
    "vision_flush",
    "group_prepare",
    "group_propose",
    "group_apply",
    "local_refinement",
    "sidestep",
    "attack_position_search",
    "index_collision",
    "index_chunks",
    "other",
];
/// The last name: the phases of a tick recorded under a name not listed above.
pub(super) const OTHER: usize = NAMES.len() - 1;

fn listed(name: &str) -> Option<usize> {
    NAMES.iter().position(|key| *key == name)
}

type Counters = Arc<[AtomicU64; NAMES.len()]>;

#[derive(Debug)]
pub(super) struct Phases(Counters);

impl Default for Phases {
    fn default() -> Self {
        Self(Arc::new(std::array::from_fn(|_| AtomicU64::new(0))))
    }
}

impl Phases {
    /// A phase of the tick. One that is not listed still counts, in `other`,
    /// so no recorded time is lost. The names need not add up to the tick: a
    /// listed one nested in another overlaps it, and work between two phases
    /// is in none.
    pub(super) fn record(&self, name: &'static str, elapsed: Duration) {
        self.add(listed(name).unwrap_or(OTHER), elapsed);
    }
    fn add(&self, at: usize, elapsed: Duration) {
        // Individual index updates can take less than a microsecond. Round
        // only their accumulated tick total, rather than losing each one.
        self.0[at].fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }
    pub(super) fn drain(&self) -> Micros {
        Micros(std::array::from_fn(|at| {
            self.0[at].swap(0, Ordering::Relaxed) / 1_000
        }))
    }
    pub(super) fn span(&self, name: &'static str) -> Measurement {
        Measurement {
            counters: self.0.clone(),
            name,
            started: Instant::now(),
        }
    }
}

#[derive(Clone)]
pub(super) struct Micros([u64; NAMES.len()]);
impl Default for Micros {
    fn default() -> Self {
        Self([0; NAMES.len()])
    }
}
impl Micros {
    pub(super) fn values(&self) -> impl Iterator<Item = (&'static str, u64)> + '_ {
        NAMES.iter().copied().zip(self.0)
    }
}
impl Serialize for Micros {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(NAMES.len()))?;
        for (name, value) in NAMES.iter().zip(self.0) {
            map.serialize_entry(name, &value)?;
        }
        map.end()
    }
}
impl std::fmt::Debug for Micros {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(NAMES.iter().zip(self.0).filter(|(_, value)| *value != 0))
            .finish()
    }
}

pub(super) struct Measurement {
    counters: Counters,
    name: &'static str,
    started: Instant,
}
impl aoe_map::NavigationSpan for Measurement {}
impl Drop for Measurement {
    fn drop(&mut self) {
        // A span runs inside a listed phase: in `other` it would count twice.
        if let Some(at) = listed(self.name) {
            Phases(self.counters.clone()).add(at, self.started.elapsed());
        }
    }
}
