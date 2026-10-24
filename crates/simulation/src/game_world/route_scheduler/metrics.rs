//! Cumulative route-demand counters (diagnostics only: never hashed, never
//! read by the simulation). The server logs them with the scheduler line so
//! demand can be compared with throughput in a live match.
use std::fmt;

/// Work buckets: below 256, 1 Ki, 4 Ki, 16 Ki, 64 Ki, 256 Ki, and above.
pub(crate) const WORK_BUCKETS: usize = 7;

fn bucket(work: u32) -> usize {
    match work {
        0..256 => 0,
        256..1_024 => 1,
        1_024..4_096 => 2,
        4_096..16_384 => 3,
        16_384..65_536 => 4,
        65_536..262_144 => 5,
        _ => 6,
    }
}

/// Distance buckets (Chebyshev tiles): up to 4, 16, 64, 256, and above.
fn distance_bucket(tiles: i32) -> usize {
    match tiles {
        ..=4 => 0,
        5..=16 => 1,
        17..=64 => 2,
        65..=256 => 3,
        _ => 4,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouteMetrics {
    /// Orders that reached route planning.
    pub orders: u64,
    /// Orders answered when issued (path, cache or fast local search).
    pub solved_at_issue: u64,
    pub cache_hits: u64,
    pub fast_path: u64,
    /// Orders that had to wait for the fair pass, by straight distance.
    pub deferred_by_distance: [u64; 5],
    /// Deferred orders that got no planner (no slot) when issued.
    pub deferred_without_slot: u64,
    /// Work spent: at issue, in the fair pass, and planning ahead.
    pub issue_work: u64,
    pub fair_work: u64,
    pub lookahead_work: u64,
    /// Fair-pass polls refused a slot.
    pub slot_refusals: u64,
    /// Searches that found their first leg, by the work it took.
    pub found: [u64; WORK_BUCKETS],
    /// Searches that ended without a route, by the work they spent.
    pub failed: [u64; WORK_BUCKETS],
    /// Searches ended by the stuck-search bound.
    pub stuck: u64,
    pub yielded: u64,
    /// Near routes given up on for the closest reachable tile.
    pub approached: u64,
    /// The latest searches that ended stuck or needed 64 Ki work or more.
    pub samples: std::collections::VecDeque<String>,
}

impl RouteMetrics {
    pub(crate) fn sample(
        &mut self,
        what: &str,
        kind: crate::UnitKind,
        planner: &aoe_map::RoutePlanner,
    ) {
        if self.samples.len() >= 8 {
            self.samples.pop_front();
        }
        let (o, d) = (planner.origin(), planner.destination());
        let area = planner.explored_area();
        self.samples.push_back(format!(
            "{what} {kind:?} ({},{})->({},{}) work {} area {}x{} certified {:?}",
            o.x,
            o.y,
            d.x,
            d.y,
            planner.work(),
            area.max.x - area.min.x,
            area.max.y - area.min.y,
            planner.certified_destination()
        ));
    }

    pub(crate) fn found(&mut self, work: u32) {
        self.found[bucket(work)] += 1;
    }

    pub(crate) fn failed(&mut self, work: u32) {
        self.failed[bucket(work)] += 1;
    }

    pub(crate) fn deferred(&mut self, from: aoe_core::TileCoord, to: aoe_core::TileCoord) {
        let tiles = (from.x - to.x).abs().max((from.y - to.y).abs());
        self.deferred_by_distance[distance_bucket(tiles)] += 1;
    }
}

impl fmt::Display for RouteMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "orders={} at_issue={} cache_hits={} fast={} deferred={:?} no_slot={} \
             work issue={} fair={} ahead={} refusals={} found={:?} failed={:?} stuck={} yielded={} approached={} \
             samples={:?}",
            self.orders,
            self.solved_at_issue,
            self.cache_hits,
            self.fast_path,
            self.deferred_by_distance,
            self.deferred_without_slot,
            self.issue_work,
            self.fair_work,
            self.lookahead_work,
            self.slot_refusals,
            self.found,
            self.failed,
            self.stuck,
            self.yielded,
            self.approached,
            self.samples,
        )
    }
}

/// Environment-owned timing observer; measurements never enter replay state.
pub trait PlanningProbe: std::fmt::Debug + Send + Sync {
    fn begin(&self);
    fn end(&self);
    fn cache_span(&self) -> Option<Box<dyn aoe_map::NavigationSpan>> {
        None
    }
}

pub(crate) struct PlanningMeasurement<'a>(pub(crate) &'a dyn PlanningProbe);
impl Drop for PlanningMeasurement<'_> {
    fn drop(&mut self) {
        self.0.end();
    }
}
