//! Cumulative gathering work counters: where the phase's cost goes, as
//! deterministic counts a benchmark divides by ticks. Never hashed.
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Every count is cumulative since the world was made.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GatherWork {
    /// Gatherer visits, and those a sleep or a drop-off wait skipped.
    pub visits: u64,
    pub slept: u64,
    /// Visits skipped because the gatherer walks under an order.
    pub walkers: u64,
    /// Full `gather_step` calls.
    pub steps: u64,
    /// Drop-off choices (`begin_return`), the buildings and perimeter tiles
    /// they ranked, and the routes they tried.
    pub returns: u64,
    pub return_buildings: u64,
    pub return_tiles: u64,
    pub return_routes: u64,
    /// Replacement-resource searches and the nodes they ranked.
    pub replacements: u64,
    pub replacement_candidates: u64,
    /// Work-spot queries (`resource_work_positions`).
    pub work_spots: u64,
    /// Failed issued drop-off attempts: unreachable, invalid, or other errors.
    /// Candidates rejected before issue are counted separately below.
    pub return_unreachable: u64,
    pub return_invalid: u64,
    pub return_other: u64,
    /// Candidates rejected by an affordable cached or sealed-source proof,
    /// before issuing a movement order. Logical route work is still charged.
    pub return_known_unreachable: u64,
    /// Farmer steps that found FARM had not produced the next food yet.
    pub farm_waits: u64,
    /// Drop-off choices by farmers, and their route attempts.
    pub farm_returns: u64,
    pub farm_return_routes: u64,
}

/// The counters, one relaxed atomic each so shared readers count too.
#[derive(Debug, Default)]
pub(crate) struct GatherCounters([AtomicU64; 18]);

#[derive(Clone, Copy)]
pub(crate) enum Count {
    Visits,
    Slept,
    Walkers,
    Steps,
    Returns,
    ReturnBuildings,
    ReturnTiles,
    ReturnRoutes,
    Replacements,
    ReplacementCandidates,
    WorkSpots,
    ReturnUnreachable,
    ReturnInvalid,
    ReturnOther,
    FarmWaits,
    FarmReturns,
    FarmReturnRoutes,
    ReturnKnownUnreachable,
}

impl crate::GameWorld {
    pub fn gather_work(&self) -> GatherWork {
        let c = |count: Count| self.economy.gather_work.0[count as usize].load(Relaxed);
        GatherWork {
            visits: c(Count::Visits),
            slept: c(Count::Slept),
            walkers: c(Count::Walkers),
            steps: c(Count::Steps),
            returns: c(Count::Returns),
            return_buildings: c(Count::ReturnBuildings),
            return_tiles: c(Count::ReturnTiles),
            return_routes: c(Count::ReturnRoutes),
            replacements: c(Count::Replacements),
            replacement_candidates: c(Count::ReplacementCandidates),
            work_spots: c(Count::WorkSpots),
            return_unreachable: c(Count::ReturnUnreachable),
            return_invalid: c(Count::ReturnInvalid),
            return_other: c(Count::ReturnOther),
            return_known_unreachable: c(Count::ReturnKnownUnreachable),
            farm_waits: c(Count::FarmWaits),
            farm_returns: c(Count::FarmReturns),
            farm_return_routes: c(Count::FarmReturnRoutes),
        }
    }

    pub(crate) fn count_gather(&self, count: Count, n: u64) {
        self.economy.gather_work.0[count as usize].fetch_add(n, Relaxed);
    }
}
