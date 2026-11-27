//! The terminal table and the JSON report (`schema` 1, documented in
//! docs/scale/game-score.md).
mod table;
#[cfg(test)]
mod tests;

pub(super) use table::table;

use super::Plan;
use super::judge::Verdict;
use super::measure::Measurement;
use super::scenarios::Scenario;
use serde_json::{Value, json};

/// One scenario as it was played: its runs (one per worker count) and what
/// comparing them showed.
pub(super) struct Played {
    pub scenario: &'static Scenario,
    /// The plan of the first run: the others differ by worker count only.
    pub plan: Plan,
    pub runs: Vec<Measurement>,
    pub verdict: Verdict,
}

fn start(played: &Played) -> Value {
    match played.scenario.late_percent(played.plan.late_percent) {
        None => json!({ "kind": "fresh" }),
        Some(percent) => json!({ "kind": "late_game", "percent": percent }),
    }
}

fn run(run: &Measurement) -> Value {
    json!({
        "workers": run.workers,
        "native_seconds": run.native_seconds,
        "setup_seconds": run.setup_seconds,
        "tick_seconds": run.tick_seconds,
        "ticks_per_second": run.ticks_per_second,
        "slowest_tick": run.slowest_tick,
        "peak_memory_mib": run.peak_memory_mib,
        "orders": run.order_count,
        "final_hash": run.digests.last().map(|digest| digest.hash.clone()),
    })
}

fn scenario(played: &Played) -> Value {
    let base = played.runs.first();
    json!({
        "id": played.scenario.id,
        "what": played.scenario.what,
        "note": played.scenario.note,
        "start": start(played),
        "seats": played.plan.seats,
        "seed": played.plan.seed,
        "ticks": played.plan.ticks,
        "hash_every": played.plan.hash_every,
        "sample_every": played.plan.sample_every,
        // The numbers below come from the first run, at these workers.
        "measured_at_workers": base.map(|run| run.workers),
        "windows": base.map(|run| &run.windows),
        "series": base.map(|run| &run.series),
        "determinism": {
            "holds": played.verdict.holds,
            "fixed_workers": played.scenario.fixed_workers,
            "runs": played.runs.iter().map(run).collect::<Vec<_>>(),
            "against": played.verdict.against,
            "diagnosis": played.verdict.diagnosis,
        },
    })
}

/// The JSON report: `schema`, `header`, then one object per scenario.
pub(super) fn json(header: Value, played: &[Played], table: &str) -> Value {
    json!({
        "schema": 1,
        "name": "game score",
        "header": header,
        "scenarios": played.iter().map(scenario).collect::<Vec<_>>(),
        "table": table,
    })
}
