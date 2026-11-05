//! Opt-in inclusive movement spans through the simulation's existing probe.
use aoe_simulation::PlanningProbe;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

const COUNT: usize = 7 + PLANNING.len();
const PLANNING: [&str; 12] = [
    "plan_serve",
    "plan_fast",
    "plan_fair",
    "plan_ahead",
    "search",
    "hierarchy",
    "components",
    "pocket",
    "portals",
    "portal_costs",
    "terrain",
    "terrain_point",
];
const DETAIL: [&str; 7] = [
    "avoidance_query",
    "local_refinement",
    "sidestep",
    "arrival_search",
    "group_prepare",
    "group_propose",
    "group_apply",
];
fn name_at(name: &str) -> Option<usize> {
    DETAIL
        .iter()
        .chain(&PLANNING)
        .position(|&candidate| candidate == name)
}
#[derive(Debug, Default)]
pub(super) struct MovementProbe {
    coarse: bool,
    nanos: Arc<[AtomicU64; COUNT]>,
    calls: Arc<[AtomicU64; COUNT]>,
}
struct Span {
    nanos: Arc<[AtomicU64; COUNT]>,
    calls: Arc<[AtomicU64; COUNT]>,
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
        let at = name_at(name)?;
        if at < 7 && (at >= 4) != self.coarse {
            return None;
        }
        Some(self.span(at))
    }
}
impl aoe_map::NavigationProbe for MovementProbe {
    fn begin(&self, phase: aoe_map::NavigationPhase) -> Box<dyn aoe_map::NavigationSpan> {
        // Every phase has a counter or shares the last; never panics on a new one.
        self.span(name_at(phase.name()).unwrap_or(COUNT - 1))
    }
}
impl MovementProbe {
    fn span(&self, at: usize) -> Box<Span> {
        Box::new(Span {
            nanos: self.nanos.clone(),
            calls: self.calls.clone(),
            at,
            start: Instant::now(),
        })
    }
    pub(super) fn coarse() -> Self {
        Self {
            coarse: true,
            ..Self::default()
        }
    }
    pub(super) fn drain(&self) -> Value {
        DETAIL
            .iter()
            .chain(&PLANNING)
            .enumerate()
            .filter(|(at, _)| *at >= 7 || (*at >= 4) == self.coarse)
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
