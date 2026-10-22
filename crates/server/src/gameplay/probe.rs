//! Wall-clock cost of the last tick's fan-out, for the scale benchmark.
use super::GameplayService;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(super) struct FanoutProbe {
    build_ns: AtomicU64,
    deliver_ns: AtomicU64,
}

impl FanoutProbe {
    pub(super) fn record(&self, build: std::time::Duration, deliver: std::time::Duration) {
        self.build_ns
            .store(build.as_nanos() as u64, Ordering::Relaxed);
        self.deliver_ns
            .store(deliver.as_nanos() as u64, Ordering::Relaxed);
    }
}

impl GameplayService {
    /// Milliseconds the last tick spent after the simulation advanced, as
    /// `(build, deliver)`: reading the world to build every session's view,
    /// then handing the views to the sessions.
    pub fn last_fanout_timing(&self) -> (f64, f64) {
        (
            self.fanout_probe.build_ns.load(Ordering::Relaxed) as f64 / 1e6,
            self.fanout_probe.deliver_ns.load(Ordering::Relaxed) as f64 / 1e6,
        )
    }
}
