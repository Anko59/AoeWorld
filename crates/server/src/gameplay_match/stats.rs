//! Per-tick timing of the running match, read by the scale harness
//! (`GET /match/stats`, admin secret). Recording is a lock and a push per
//! tick; nothing here reads simulation state beyond a unit count.
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// Ticks kept (100 minutes at 20 Hz); older ones are dropped.
pub(crate) const MAX_RECORDS: usize = 120_000;

/// What `GameplayService::tick` learned about itself, handed to `run_ticks`.
#[derive(Default)]
pub struct TickProbe {
    advance_micros: AtomicU64,
    orders: AtomicU64,
}

impl TickProbe {
    pub(crate) fn record(&self, advance: Duration, orders: usize) {
        self.advance_micros
            .store(advance.as_micros() as u64, Ordering::Relaxed);
        self.orders.store(orders as u64, Ordering::Relaxed);
    }

    fn read(&self) -> (u64, u64) {
        (
            self.advance_micros.load(Ordering::Relaxed),
            self.orders.load(Ordering::Relaxed),
        )
    }
}

/// One tick: `[tick, total_us, sim_us, late_us, orders, units, missed]`.
#[derive(Clone, Copy, Serialize)]
pub(crate) struct TickRecord(u64, u64, u64, u64, u64, u64, u8);

#[derive(Default)]
struct Inner {
    /// Sequence number of the oldest kept record.
    first: u64,
    records: VecDeque<TickRecord>,
    missed: u64,
}

#[derive(Default)]
pub struct TickStats {
    inner: Mutex<Inner>,
}

#[derive(Serialize)]
pub(crate) struct StatsPage {
    /// Pass back as `since` to continue.
    next: u64,
    missed_total: u64,
    /// `[tick, total_us, sim_us, late_us, orders, units, missed]` per tick.
    ticks: Vec<TickRecord>,
}

impl TickStats {
    /// Records a finished tick. `total` is the whole tick (orders, simulation
    /// and fan-out), `late` how long after its schedule it started.
    pub(crate) fn record(
        &self,
        tick: u64,
        total: Duration,
        late: Duration,
        period: Duration,
        probe: &TickProbe,
        units: usize,
    ) {
        let (sim, orders) = probe.read();
        let missed = late > period || total > period;
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.missed += u64::from(missed);
        inner.records.push_back(TickRecord(
            tick,
            total.as_micros() as u64,
            sim,
            late.as_micros() as u64,
            orders,
            units as u64,
            u8::from(missed),
        ));
        if inner.records.len() > MAX_RECORDS {
            inner.records.pop_front();
            inner.first += 1;
        }
    }

    pub(crate) fn page(&self, since: u64) -> StatsPage {
        let Ok(inner) = self.inner.lock() else {
            return StatsPage {
                next: since,
                missed_total: 0,
                ticks: Vec::new(),
            };
        };
        let skip = since.saturating_sub(inner.first) as usize;
        StatsPage {
            next: inner.first + inner.records.len() as u64,
            missed_total: inner.missed,
            ticks: inner.records.iter().skip(skip).copied().collect(),
        }
    }
}

impl crate::AppState {
    /// Records the tick `gameplay` just ran, scheduled for `scheduled` and
    /// started at `started`; a missed deadline is counted for `/health`.
    pub(crate) async fn observe_tick(
        &self,
        gameplay: &crate::GameplayService,
        scheduled: tokio::time::Instant,
        started: tokio::time::Instant,
    ) {
        let (late, total) = (started.duration_since(scheduled), started.elapsed());
        let (tick, units) = gameplay.tick_and_units().await;
        let probe = gameplay.probe();
        self.tick_stats
            .record(tick, total, late, self.tick_period, &probe, units);
        if late > self.tick_period || total > self.tick_period {
            self.tick_deadline_misses.fetch_add(1, Ordering::Relaxed);
        }
    }
}
