//! Fixed storage per tick: profiling must not allocate a tree for every sample.
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

const NAMES: [&str; 30] = [
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
];
type Counters = Arc<[AtomicU64; NAMES.len()]>;

#[derive(Debug, Default)]
pub(super) struct Phases(Counters);

impl Phases {
    pub(super) fn record(&self, name: &'static str, elapsed: Duration) {
        if let Some(at) = NAMES.iter().position(|key| *key == name) {
            self.0[at].fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
        }
    }
    pub(super) fn drain(&self) -> Micros {
        Micros(std::array::from_fn(|at| {
            self.0[at].swap(0, Ordering::Relaxed)
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

#[derive(Clone, Default)]
pub(super) struct Micros([u64; NAMES.len()]);
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
        Phases(self.counters.clone()).record(self.name, self.started.elapsed());
    }
}
