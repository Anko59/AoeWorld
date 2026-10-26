//! Opt-in inclusive movement spans through the simulation's existing probe.
use aoe_simulation::PlanningProbe;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

const NAMES: [&str; 4] = [
    "avoidance_query",
    "local_refinement",
    "sidestep",
    "arrival_search",
];
#[derive(Debug, Default)]
pub(super) struct MovementProbe {
    nanos: Arc<[AtomicU64; 4]>,
    calls: Arc<[AtomicU64; 4]>,
}
struct Span {
    nanos: Arc<[AtomicU64; 4]>,
    calls: Arc<[AtomicU64; 4]>,
    at: usize,
    start: Instant,
}
impl aoe_map::NavigationSpan for Span {}
impl Drop for Span {
    fn drop(&mut self) {
        self.nanos[self.at].fetch_add(self.start.elapsed().as_nanos() as u64, Ordering::Relaxed);
        self.calls[self.at].fetch_add(1, Ordering::Relaxed);
    }
}
impl PlanningProbe for MovementProbe {
    fn begin(&self) {}
    fn end(&self) {}
    fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
        Some(Box::new(Span {
            nanos: self.nanos.clone(),
            calls: self.calls.clone(),
            at: NAMES.iter().position(|&n| n == name)?,
            start: Instant::now(),
        }))
    }
}
impl MovementProbe {
    pub(super) fn drain(&self) -> Value {
        NAMES
            .iter()
            .enumerate()
            .map(|(at, &name)| {
                (
                    name.to_owned(),
                    json!({
                        "inclusive_ms": self.nanos[at].swap(0, Ordering::Relaxed) as f64 / 1e6,
                        "calls": self.calls[at].swap(0, Ordering::Relaxed),
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>()
            .into()
    }
}
