//! The terminal table and the JSON report: every number named in plain words.
use super::judge::{Comparison, Verdict};
use super::measure::Measurement;
use serde_json::{Value, json};

type Row = (&'static str, fn(&Measurement) -> String);

/// What the table shows, in order. Every row is one number of the run.
const ROWS: [Row; 20] = [
    ("ticks per second", |r| format!("{:.2}", r.ticks_per_second)),
    ("slowest tick (ms)", |r| {
        format!("{:.0}", r.tick_ms["max_ms"].as_f64().unwrap_or(0.0))
    }),
    ("slowest tick was tick number", |r| {
        r.slowest_tick.to_string()
    }),
    ("peak memory (MiB)", |r| r.peak_memory_mib.to_string()),
    ("seconds to start the match", |r| {
        format!("{:.1}", r.setup_seconds)
    }),
    ("units alive at the end", |r| r.counters.units.to_string()),
    ("buildings standing at the end", |r| {
        r.counters.buildings.to_string()
    }),
    ("villagers at the end", |r| r.counters.villagers.to_string()),
    ("villagers standing idle at the end", |r| {
        r.counters.idle_villagers.to_string()
    }),
    ("movers waiting for a route, at the end", |r| {
        r.counters.waiting_for_route.to_string()
    }),
    ("movers waiting for a route, most at a sample", |r| {
        r.counters.waiting_for_route_peak.to_string()
    }),
    (
        "wait for a route, 95th percentile (ticks), at the end",
        |r| r.counters.wait_p95_ticks.to_string(),
    ),
    (
        "wait for a route, 95th percentile (ticks), worst sample",
        |r| r.counters.wait_p95_ticks_peak.to_string(),
    ),
    ("route orders given", |r| {
        r.counters.route_orders.to_string()
    }),
    ("route searches started", |r| {
        r.counters.route_searches.to_string()
    }),
    ("route orders answered when given", |r| {
        r.counters.solved_at_issue.to_string()
    }),
    ("route answers taken from the cache", |r| {
        r.counters.cache_hits.to_string()
    }),
    ("hits landed", |r| r.counters.hits.to_string()),
    ("kills / losses", |r| {
        format!("{} / {}", r.counters.kills, r.counters.losses)
    }),
    ("resources delivered in the last game minute", |r| {
        r.counters
            .delivered_last_minute
            .iter()
            .sum::<u64>()
            .to_string()
    }),
];

fn workers(count: usize) -> String {
    match count {
        1 => "1 worker".to_owned(),
        n => format!("{n} workers"),
    }
}

/// One line in words about one comparison.
fn describe(comparison: &Comparison) -> String {
    let pair = format!(
        "{} and {}",
        workers(comparison.base_workers),
        workers(comparison.workers)
    );
    if comparison.holds() {
        let orders = match comparison.same_orders {
            Some(_) => "the same hash at every hashed tick and the same orders",
            None => "the same hash at every hashed tick (no order log kept)",
        };
        return format!("{pair}: {orders}");
    }
    let tick = comparison
        .first_differing_tick
        .map_or_else(|| "unknown".to_owned(), |tick| tick.to_string());
    let mut text = format!("{pair}: DIFFERENT GAMES, first differing tick {tick}");
    if let Some(difference) = &comparison.hashes {
        text.push_str(&format!("; world: {}", difference.what));
    }
    if let Some(difference) = &comparison.orders {
        text.push_str(&format!(
            "; first differing order is number {}",
            difference.index
        ));
    }
    text
}

/// The table: one column per run, one row per number, then the verdict.
pub(super) fn table(title: &str, runs: &[Measurement], verdict: &Verdict) -> String {
    let label = ROWS
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(0)
        .max(12);
    let mut text = format!("{title}\n\n{:label$}", "");
    for run in runs {
        text.push_str(&format!("  {:>12}", workers(run.workers)));
    }
    text.push('\n');
    for (name, value) in ROWS {
        text.push_str(&format!("{name:label$}"));
        for run in runs {
            text.push_str(&format!("  {:>12}", value(run)));
        }
        text.push('\n');
    }
    text.push('\n');
    for comparison in &verdict.against {
        text.push_str(&format!("determinism, {}\n", describe(comparison)));
        if !comparison.same_counters {
            text.push_str("  the counters of the two runs differ\n");
        }
    }
    if let Some(diagnosis) = &verdict.diagnosis {
        text.push_str(&format!(
            "replay with a hash at every tick: {}\n",
            describe(diagnosis)
        ));
    }
    if verdict.against.is_empty() {
        text.push_str("determinism: not checked (one run only)\n");
    }
    text
}

/// The JSON report.
pub(super) fn json(header: Value, runs: &[Measurement], verdict: &Verdict, table: &str) -> Value {
    json!({
        "version": 1,
        "name": "game score",
        "header": header,
        "determinism": verdict,
        "runs": runs,
        "table": table,
    })
}
