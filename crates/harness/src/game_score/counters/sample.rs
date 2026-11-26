//! The world at a sampled tick: the census (one pass over every unit) and the
//! engine's cumulative counters. A window reads the samples at its two ends
//! (differences of the cumulative ones) and between them (peaks and means).
use crate::perf_scale::stats::memory;
use aoe_core::PlayerId;
use aoe_simulation::{GameWorld, OrderDemand, OrderIssuer};
use serde::{Deserialize, Serialize};

/// Names of the order issuers, in the order of `OrderIssuer`.
pub(in crate::game_score) const ISSUERS: [&str; 8] = [
    "player",
    "python_ai",
    "auto_scout",
    "steward",
    "vassal",
    "general",
    "automation",
    "rust_ai",
];

/// What the engine counted for the orders of one issuer, since the start.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct IssuerTotals {
    /// Decisions that led to orders (a trip started, a manager order).
    pub decisions: u64,
    /// Movement orders asked for, and those that changed the unit's route.
    pub orders: u64,
    pub accepted: u64,
    /// Fresh route searches those orders started.
    pub route_searches: u64,
}

impl From<&OrderDemand> for IssuerTotals {
    fn from(demand: &OrderDemand) -> Self {
        Self {
            decisions: demand.decisions,
            orders: demand.orders,
            accepted: demand.accepted,
            route_searches: demand.route_searches,
        }
    }
}

/// The route scheduler's cumulative counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct RouteTotals {
    pub orders: u64,
    pub searches_started: u64,
    pub solved_at_issue: u64,
    pub cache_hits: u64,
    pub stuck: u64,
    pub expired: u64,
    pub approached: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Sample {
    pub tick: u64,
    pub units: u64,
    pub buildings: u64,
    pub villagers: u64,
    pub idle_villagers: u64,
    /// Movers waiting for a route, the 95th percentile of how long they had
    /// waited so far, and the longest wait (ticks).
    pub waiting: u64,
    pub wait_p95_ticks: u64,
    pub oldest_wait_ticks: u64,
    pub routes: RouteTotals,
    pub issuers: [IssuerTotals; 8],
    /// Kills and losses of every seat since the start (a loss may be a
    /// building).
    pub kills: u64,
    pub losses: u64,
    /// Buildings whose id showed for the first time, since the start.
    pub buildings_born: u64,
    /// Resident memory of the process now, MiB: wall-clock noise, not game.
    pub resident_mib: u64,
}

/// The sample of `world`; `buildings_born` is the running count the caller
/// keeps (ids above the highest seen so far).
pub(in crate::game_score) fn capture(
    world: &GameWorld,
    seats: usize,
    buildings_born: u64,
) -> Sample {
    let census = world.state_census();
    let stats = world.route_scheduler_stats();
    let metrics = &stats.metrics;
    let (mut kills, mut losses) = (0, 0);
    for seat in 0..seats {
        let (killed, lost) = world.player_combat_totals(PlayerId(seat as u16));
        kills += killed;
        losses += lost;
    }
    let mut issuers = [IssuerTotals::default(); 8];
    for (total, demand) in issuers.iter_mut().zip(world.order_demand()) {
        *total = demand.into();
    }
    debug_assert_eq!(OrderIssuer::RustAi as usize, ISSUERS.len() - 1);
    Sample {
        tick: world.tick().0,
        units: census.units,
        buildings: world.buildings().count() as u64,
        villagers: census.villagers,
        idle_villagers: census.idle_villagers,
        waiting: census.waiting,
        wait_p95_ticks: census.wait_p95_ticks,
        oldest_wait_ticks: stats.oldest_wait_ticks,
        routes: RouteTotals {
            orders: metrics.orders,
            searches_started: metrics.searches_started,
            solved_at_issue: metrics.solved_at_issue,
            cache_hits: metrics.cache_hits,
            stuck: metrics.stuck,
            expired: metrics.expired,
            approached: metrics.approached,
        },
        issuers,
        kills,
        losses,
        buildings_born,
        resident_mib: memory()["rss_mib"].as_u64().unwrap_or(0),
    }
}
