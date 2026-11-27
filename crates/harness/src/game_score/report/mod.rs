//! The terminal table and the JSON report (`schema` 1, documented in
//! docs/scale/game-score.md).
mod staged;
mod table;
#[cfg(test)]
mod tests;

pub(super) use table::table;

use super::Plan;
use super::judge::Verdict;
use super::measure::Measurement;
use super::scenarios::{Scenario, Start};
use serde_json::{Value, json};

/// The version of the report's layout (`schema`); a reader that compares two reports checks it.
pub(super) const SCHEMA: u64 = 1;

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
    if let Start::Staged(staging) = played.scenario.start {
        let armies: Vec<Value> = staging
            .armies
            .iter()
            .map(|army| json!({ "seat": army.seat, "from": format!("{:?}", army.from), "units": army.units() }))
            .collect();
        return json!({
            "kind": "staged",
            "choke": staging.choke.name,
            "attack": staging.attack,
            "units": staging.units(),
            "armies": armies,
            "choke_record": {
                "saddle": staging.choke.saddle,
                "ends": staging.choke.ends,
                "width_tiles": staging.choke.width_tiles,
                "length_tiles": staging.choke.length_tiles,
                "flow": staging.choke.flow,
                "near": staging.choke.near,
                "far": staging.choke.far,
                "sides": staging.choke.sides,
                "why": staging.choke.why,
            },
        });
    }
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
        "schema": SCHEMA,
        "name": "game score",
        "header": header,
        "scenarios": played.iter().map(scenario).collect::<Vec<_>>(),
        "table": table,
    })
}
