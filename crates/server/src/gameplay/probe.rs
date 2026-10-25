//! Wall-clock cost of the last tick's fan-out, for the scale benchmark.
use super::GameplayService;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

type World = tokio::sync::RwLock<aoe_simulation::GameWorld>;
static WORLDS: std::sync::Mutex<Vec<std::sync::Weak<World>>> = std::sync::Mutex::new(Vec::new());

pub(super) fn track(world: &std::sync::Arc<World>) {
    let mut worlds = WORLDS.lock().unwrap_or_else(|error| error.into_inner());
    worlds.retain(|world| world.strong_count() > 0);
    worlds.push(std::sync::Arc::downgrade(world));
}

/// Allocation capacities, not RSS: allocator slack, stacks and in-flight work
/// are deliberately left to the process/container measurements.
#[derive(serde::Serialize)]
pub(crate) struct MemoryUsage {
    worlds: usize,
    world_references: usize,
    sessions: usize,
    tick: u64,
    units: usize,
    snapshot_bytes: usize,
    snapshot_chunks: usize,
    prepared_atlas_entries: usize,
    walk_cache: CacheUsage,
    portal_cache: CacheUsage,
    resource_cache: CacheUsage,
    route_cache: CacheUsage,
}

#[derive(Default, serde::Serialize)]
struct CacheUsage {
    entries: usize,
    retained_bytes: usize,
}

#[derive(Default)]
pub(super) struct FanoutProbe {
    nanos: [AtomicU64; 4],
}

impl FanoutProbe {
    /// Records the phases in order: per-viewer state, the shared unit frame,
    /// the per-session views, and delivery under the session lock.
    pub(super) fn record(&self, phases: [Duration; 4]) {
        for (slot, phase) in self.nanos.iter().zip(phases) {
            slot.store(phase.as_nanos() as u64, Ordering::Relaxed);
        }
    }
}

/// Milliseconds the last tick spent after the simulation advanced.
#[derive(Clone, Copy, Debug, Default)]
pub struct FanoutTiming {
    pub viewers_ms: f64,
    pub frame_ms: f64,
    pub sessions_ms: f64,
    pub deliver_ms: f64,
}

impl FanoutTiming {
    /// Reading the world: everything but handing views to the sessions.
    pub fn build_ms(&self) -> f64 {
        self.viewers_ms + self.frame_ms + self.sessions_ms
    }
}

impl GameplayService {
    pub(crate) async fn memory_usage(&self) -> MemoryUsage {
        let worlds = WORLDS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|world| world.strong_count() > 0)
            .count();
        let world = self.world.read().await;
        let mut usage = MemoryUsage {
            worlds,
            world_references: std::sync::Arc::strong_count(&self.world),
            sessions: self.sessions.lock().await.len(),
            tick: world.tick().0,
            units: world.unit_count(),
            snapshot_bytes: 0,
            snapshot_chunks: 0,
            prepared_atlas_entries: 0,
            walk_cache: CacheUsage::default(),
            portal_cache: CacheUsage::default(),
            resource_cache: {
                let (entries, retained_bytes) = world.resource_cache_usage();
                CacheUsage {
                    entries,
                    retained_bytes,
                }
            },
            route_cache: {
                let cache = world.navigation_cache_usage();
                CacheUsage {
                    entries: cache.entries,
                    retained_bytes: cache.retained_bytes,
                }
            },
        };
        if let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() {
            (usage.snapshot_bytes, usage.snapshot_chunks) = generator.static_navigation_usage();
            usage.prepared_atlas_entries = generator.prepared_navigation_entries();
            let (retained_bytes, entries) = generator.walk_cache_usage();
            usage.walk_cache = CacheUsage {
                entries,
                retained_bytes,
            };
            let (retained_bytes, entries) = generator.portal_cache_usage();
            usage.portal_cache = CacheUsage {
                entries,
                retained_bytes,
            };
        }
        usage
    }

    pub fn last_fanout_timing(&self) -> FanoutTiming {
        let ms = |i: usize| self.fanout_probe.nanos[i].load(Ordering::Relaxed) as f64 / 1e6;
        FanoutTiming {
            viewers_ms: ms(0),
            frame_ms: ms(1),
            sessions_ms: ms(2),
            deliver_ms: ms(3),
        }
    }
}

#[path = "probe/tests.rs"]
#[cfg(test)]
mod tests;
