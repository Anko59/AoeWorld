//! What a window cost, and the cost per unit of service. Wall time and memory
//! are noisy: they never take part in the comparison of runs.
use super::service::{Record, Service};
use crate::perf_scale::stats::Series;
use serde::{Deserialize, Serialize};

/// Milliseconds one tick took inside `tick()`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct TickMs {
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Cost {
    /// Instructions counted for the window: `null` until the Callgrind child
    /// exists (docs/scale/game-score.md).
    pub instructions: Option<u64>,
    /// Seconds spent inside `tick()` in the window.
    pub tick_seconds: f64,
    pub tick_ms: TickMs,
    /// The most resident memory at a sample inside the window, MiB.
    pub peak_memory_mib: u64,
}

/// Cost for each unit of service: milliseconds of tick time when the window
/// has no instruction count, instructions when it has. `basis` says which.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(in crate::game_score) struct Efficiency {
    pub basis: String,
    pub unit: String,
    pub per_arrival: Option<f64>,
    pub per_thousand_resources_delivered: Option<f64>,
    pub per_hit: Option<f64>,
    pub per_unit_produced: Option<f64>,
}

fn rounded(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// The cost of ticks `from + 1` through `to`.
pub(in crate::game_score) fn cost(record: Record<'_>, from: u64, to: u64) -> Cost {
    let rows = record
        .rows
        .get(from as usize..to as usize)
        .unwrap_or_default();
    let mut series = Series::default();
    let mut nanos = 0_u128;
    for row in rows {
        series.push_ms(u128::from(row.nanos));
        nanos += u128::from(row.nanos);
    }
    let summary = series.summary();
    let number = |key: &str| summary[key].as_f64().unwrap_or(0.0);
    Cost {
        instructions: None,
        tick_seconds: rounded(nanos as f64 / 1e9),
        tick_ms: TickMs {
            mean: number("mean_ms"),
            p50: number("p50_ms"),
            p95: number("p95_ms"),
            p99: number("p99_ms"),
            max: number("max_ms"),
        },
        peak_memory_mib: record
            .samples
            .iter()
            .filter(|sample| sample.tick >= from && sample.tick <= to)
            .map(|sample| sample.resident_mib)
            .max()
            .unwrap_or(0),
    }
}

/// How much `cost` buys in `service`.
pub(in crate::game_score) fn efficiency(cost: &Cost, service: &Service) -> Efficiency {
    let (spent, basis, unit) = match cost.instructions {
        Some(count) => (count as f64, "instructions", "instructions"),
        None => (cost.tick_seconds * 1e3, "wall_time", "milliseconds"),
    };
    let per =
        |served: u64, scale: f64| (served > 0).then(|| rounded(spent * scale / served as f64));
    Efficiency {
        basis: basis.to_owned(),
        unit: unit.to_owned(),
        per_arrival: per(service.arrival.orders_arrived, 1.0),
        per_thousand_resources_delivered: per(service.economy.delivered_total, 1000.0),
        per_hit: per(service.fights.hits, 1.0),
        per_unit_produced: per(service.size.units_produced, 1.0),
    }
}
