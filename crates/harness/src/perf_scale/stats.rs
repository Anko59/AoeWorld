//! Sample summaries and process memory for the scale benchmark.
use serde_json::{Value, json};
use std::fs;

/// Milliseconds per sample, summarised as p50/p95/p99/max/mean.
#[derive(Default, Clone)]
pub(super) struct Series(pub Vec<f64>);

impl Series {
    pub(super) fn push_ms(&mut self, nanos: u128) {
        self.0.push(nanos as f64 / 1e6);
    }

    fn percentile(sorted: &[f64], q: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        let rank = ((sorted.len() as f64 - 1.0) * q).round() as usize;
        sorted[rank.min(sorted.len() - 1)]
    }

    pub(super) fn summary(&self) -> Value {
        let mut sorted = self.0.clone();
        sorted.sort_by(f64::total_cmp);
        let mean = if sorted.is_empty() {
            0.0
        } else {
            sorted.iter().sum::<f64>() / sorted.len() as f64
        };
        json!({
            "samples": sorted.len(),
            "mean_ms": round(mean),
            "p50_ms": round(Self::percentile(&sorted, 0.50)),
            "p95_ms": round(Self::percentile(&sorted, 0.95)),
            "p99_ms": round(Self::percentile(&sorted, 0.99)),
            "max_ms": round(sorted.last().copied().unwrap_or(0.0)),
        })
    }
}

pub(super) fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Resident and peak resident bytes of this process (Linux `/proc`).
pub(super) fn memory() -> Value {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kib = |key: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|rest| {
                rest.trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0)
    };
    json!({ "rss_mib": kib("VmRSS:") / 1024, "peak_rss_mib": kib("VmHWM:") / 1024 })
}

#[cfg(test)]
mod tests;
