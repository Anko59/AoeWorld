//! Resources delivered, tick by tick, from the engine's sliding window.
//!
//! `GameWorld::recent_deliveries` adds twelve buckets of five game seconds: the
//! bucket of the current tick (a bucket is labelled with the tick that
//! credited, `tick / (5 * tick_hz)`) and the eleven before it. It is not a
//! clean minute: read at tick 600 it holds ticks 50 to 600 only, so reading it
//! every 600 ticks would leave a gap of five seconds each minute. Read after
//! every tick, the change of the window is the tick's own credits minus the
//! bucket that left it when a bucket ended, and that bucket is a sum of ticks
//! already known. `Income` keeps the twelve sums and returns each tick's
//! credits exactly, with no engine change.

const BUCKETS: usize = 12;

#[derive(Debug)]
pub(in crate::game_score) struct Income {
    /// Ticks in one bucket: five game seconds.
    bucket_ticks: u64,
    epoch: u64,
    buckets: [[u64; 4]; BUCKETS],
    last: [u64; 4],
}

impl Income {
    /// `tick_hz`: ticks per game second.
    pub fn new(tick_hz: u32) -> Self {
        Self {
            bucket_ticks: 5 * u64::from(tick_hz.max(1)),
            epoch: 0,
            buckets: [[0; 4]; BUCKETS],
            last: [0; 4],
        }
    }

    /// The credits of tick `tick` given the window `recent` read right after
    /// it (every seat together), and whether the books contradicted
    /// themselves (a window that shrank by more than the bucket that left).
    /// Call it once per tick, in order, from the tick the match starts at.
    pub fn credits(&mut self, tick: u64, recent: [u64; 4]) -> ([u64; 4], bool) {
        let epoch = tick / self.bucket_ticks;
        let mut dropped = [0; 4];
        if epoch > self.epoch {
            // One bucket per tick at most: the slot of the bucket that left
            // is the slot of the one that opens.
            let slot = (epoch % BUCKETS as u64) as usize;
            dropped = std::mem::take(&mut self.buckets[slot]);
            self.epoch = epoch;
        }
        let slot = (self.epoch % BUCKETS as u64) as usize;
        let mut credits = [0; 4];
        let mut fault = false;
        for kind in 0..4 {
            let value =
                i128::from(recent[kind]) - i128::from(self.last[kind]) + i128::from(dropped[kind]);
            fault |= value < 0;
            credits[kind] = u64::try_from(value.max(0)).unwrap_or(0);
            self.buckets[slot][kind] += credits[kind];
        }
        self.last = recent;
        (credits, fault)
    }
}
