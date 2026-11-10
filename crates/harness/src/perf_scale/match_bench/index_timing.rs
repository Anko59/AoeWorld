//! Adapter-owned clocks for relocation work inside a measured tick.
use aoe_simulation::PlanningProbe;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

pub(super) const NAMES: [&str; 2] = ["index_collision", "index_chunks"];

#[derive(Debug, Default)]
pub(super) struct IndexProbe(Arc<[AtomicU64; NAMES.len()]>);

impl IndexProbe {
    pub(super) fn drain(&self) -> [u128; NAMES.len()] {
        std::array::from_fn(|at| u128::from(self.0[at].swap(0, Ordering::Relaxed)))
    }
}

impl PlanningProbe for IndexProbe {
    fn begin(&self) {}
    fn end(&self) {}

    fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
        let at = NAMES.iter().position(|&candidate| candidate == name)?;
        Some(Box::new(Span {
            nanos: self.0.clone(),
            at,
            started: Instant::now(),
        }))
    }
}

struct Span {
    nanos: Arc<[AtomicU64; NAMES.len()]>,
    at: usize,
    started: Instant,
}

impl aoe_map::NavigationSpan for Span {}

impl Drop for Span {
    fn drop(&mut self) {
        self.nanos[self.at].fetch_add(self.started.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }
}
