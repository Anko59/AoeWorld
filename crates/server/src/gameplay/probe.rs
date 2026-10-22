//! Wall-clock cost of the last tick's fan-out, for the scale benchmark.
use super::GameplayService;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

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
