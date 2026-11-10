//! Test-only observations; no clocks or counters in the production simulation.
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Counts {
    pub(crate) calls: usize,
    pub(crate) visits: usize,
    pub(crate) elapsed: Duration,
    pub(crate) terrain_reads: usize,
    pub(crate) max_tile_reads: usize,
}

thread_local! { static COUNTS: Cell<Counts> = Cell::default(); }
thread_local! { static TILES: RefCell<BTreeMap<aoe_core::TileCoord, usize>> = RefCell::default(); }

pub(crate) fn begin_plan() {
    TILES.with(|tiles| tiles.borrow_mut().clear());
}

pub(crate) fn tile_read(tile: aoe_core::TileCoord) {
    let visits = TILES.with(|tiles| {
        let mut tiles = tiles.borrow_mut();
        let count = tiles.entry(tile).or_default();
        *count += 1;
        *count
    });
    COUNTS.with(|counts| {
        let mut c = counts.get();
        c.terrain_reads += 1;
        c.max_tile_reads = c.max_tile_reads.max(visits);
        counts.set(c);
    });
}

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
