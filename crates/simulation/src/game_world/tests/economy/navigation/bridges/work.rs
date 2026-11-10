//! Test-only observations; no clocks or counters in the production simulation.
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Counts {
    pub(crate) calls: usize,
    pub(crate) visits: usize,
    pub(crate) elapsed: Duration,
}

thread_local! { static COUNTS: Cell<Counts> = Cell::default(); }

pub(crate) fn take() -> Counts {
    COUNTS.with(|counts| counts.replace(Counts::default()))
}

pub(crate) struct Validation {
    start: Instant,
    pub(crate) visits: usize,
}

impl Validation {
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
            visits: 0,
        }
    }
}

impl Drop for Validation {
    fn drop(&mut self) {
        COUNTS.with(|counts| {
            let mut c = counts.get();
            c.calls += 1;
            c.visits += self.visits;
            c.elapsed += self.start.elapsed();
            counts.set(c);
        });
    }
}
