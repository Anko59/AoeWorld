//! Instruction counts stay in raw units, with exact integer percentiles.
use serde_json::{Value, json};

pub(super) fn summary(samples: &[u64]) -> Value {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let percentile = |q: f64| {
        let rank = ((sorted.len().saturating_sub(1)) as f64 * q).round() as usize;
        sorted.get(rank).copied().unwrap_or(0)
    };
    let sum: u128 = sorted.iter().map(|&v| u128::from(v)).sum();
    let mean = if sorted.is_empty() {
        0.0
    } else {
        sum as f64 / sorted.len() as f64
    };
    json!({"samples": sorted.len(), "mean": mean, "p50": percentile(0.50),
        "p95": percentile(0.95), "p99": percentile(0.99), "max": sorted.last().copied().unwrap_or(0)})
}
