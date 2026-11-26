//! What one run of the game score measures: numbers of the game itself, the
//! cost of the ticks, and the hashes that let two runs be compared.
use aoe_core::PlayerId;
use aoe_simulation::GameWorld;
use serde::{Deserialize, Serialize};

/// The world at one tick, kept to compare runs.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct Digest {
    pub tick: u64,
    pub hash: String,
    pub units: u64,
    pub buildings: u64,
}

/// Numbers of the game. A deterministic match gives the same ones at any
/// worker count, so they take part in the comparison of runs.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct Counters {
    pub units: u64,
    pub buildings: u64,
    pub villagers: u64,
    pub idle_villagers: u64,
    /// Movers waiting for a route, and the 95th percentile of how long they
    /// had waited so far (ticks), at the end and the worst of the samples.
    pub waiting_for_route: u64,
    pub wait_p95_ticks: u64,
    pub waiting_for_route_peak: u64,
    pub wait_p95_ticks_peak: u64,
    pub route_orders: u64,
    pub route_searches: u64,
    pub solved_at_issue: u64,
    pub cache_hits: u64,
    pub hits: u64,
    pub kills: u64,
    pub losses: u64,
    /// Food, wood, gold and stone delivered in the last sixty game seconds
    /// (the engine keeps no longer window).
    pub delivered_last_minute: [u64; 4],
}

/// The census and the deliveries at a sampled tick.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct Sample {
    pub tick: u64,
    pub units: u64,
    pub buildings: u64,
    pub villagers: u64,
    pub idle_villagers: u64,
    pub waiting_for_route: u64,
    pub wait_p95_ticks: u64,
    pub delivered_last_minute: [u64; 4],
}

/// One run in one process at one worker count.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Measurement {
    pub workers: usize,
    pub ticks: u64,
    pub seats: usize,
    /// Seconds to load the package, place the villages and seat the AIs.
    pub setup_seconds: f64,
    /// Seconds spent inside the ticks (hashing and sampling are outside).
    pub tick_seconds: f64,
    pub ticks_per_second: f64,
    /// Which tick was the slowest (1 is the first), to tell a warm-up from
    /// the steady state.
    pub slowest_tick: u64,
    /// Per-tick wall time: samples, mean, p50, p95, p99, max in milliseconds.
    pub tick_ms: serde_json::Value,
    pub peak_memory_mib: u64,
    pub counters: Counters,
    pub samples: Vec<Sample>,
    pub digests: Vec<Digest>,
    /// Orders the seats had applied and a hash of their log; `None` when the
    /// run did not record orders.
    pub order_count: Option<u64>,
    pub order_log_hash: Option<String>,
    /// Where the order log is, one line per order.
    pub order_log_file: Option<std::path::PathBuf>,
}

pub(super) fn hex(hash: [u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The world's hash and sizes now.
pub(super) fn digest(world: &GameWorld) -> Digest {
    Digest {
        tick: world.tick().0,
        hash: hex(world.canonical_hash()),
        units: world.unit_count() as u64,
        buildings: world.buildings().count() as u64,
    }
}

/// Food, wood, gold and stone delivered by every seat in the last minute.
fn delivered(world: &GameWorld, seats: usize) -> [u64; 4] {
    let mut total = [0; 4];
    for seat in 0..seats {
        let recent = world.recent_deliveries(PlayerId(seat as u16));
        for (sum, amount) in total.iter_mut().zip(recent) {
            *sum += amount;
        }
    }
    total
}

/// One pass over every unit: for the sampled ticks, not every tick.
pub(super) fn sample(world: &GameWorld, seats: usize) -> Sample {
    let census = world.state_census();
    Sample {
        tick: world.tick().0,
        units: census.units,
        buildings: world.buildings().count() as u64,
        villagers: census.villagers,
        idle_villagers: census.idle_villagers,
        waiting_for_route: census.waiting,
        wait_p95_ticks: census.wait_p95_ticks,
        delivered_last_minute: delivered(world, seats),
    }
}

/// The counters at the end of a run. `hits` is the sum of each tick's hits
/// (the engine overwrites them every tick) and `samples` the sampled ticks.
pub(super) fn counters(world: &GameWorld, seats: usize, hits: u64, samples: &[Sample]) -> Counters {
    let last = sample(world, seats);
    let routes = world.route_scheduler_stats().metrics;
    let (mut kills, mut losses) = (0, 0);
    for seat in 0..seats {
        let (killed, lost) = world.player_combat_totals(PlayerId(seat as u16));
        kills += killed;
        losses += lost;
    }
    let peak = |value: fn(&Sample) -> u64| samples.iter().map(value).chain([value(&last)]).max();
    Counters {
        units: last.units,
        buildings: last.buildings,
        villagers: last.villagers,
        idle_villagers: last.idle_villagers,
        waiting_for_route: last.waiting_for_route,
        wait_p95_ticks: last.wait_p95_ticks,
        waiting_for_route_peak: peak(|s| s.waiting_for_route).unwrap_or(0),
        wait_p95_ticks_peak: peak(|s| s.wait_p95_ticks).unwrap_or(0),
        route_orders: routes.orders,
        route_searches: routes.searches_started,
        solved_at_issue: routes.solved_at_issue,
        cache_hits: routes.cache_hits,
        hits,
        kills,
        losses,
        delivered_last_minute: last.delivered_last_minute,
    }
}
