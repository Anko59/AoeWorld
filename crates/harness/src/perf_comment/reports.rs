//! The `performance-report` artifact: the files `make perf-ci` and
//! `make perf-pressure` wrote into `reports/perf/`. Reading only; nothing
//! here measures again, and a missing file is an absent value.
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

pub(crate) struct Timing {
    pub(crate) p50: Option<u64>,
    pub(crate) p95: Option<u64>,
}

pub(crate) struct Pressure {
    pub(crate) snapshot_latency_us: Timing,
    pub(crate) max_response_backlog: Option<u64>,
    pub(crate) missed_offer_deadlines: Option<u64>,
}

/// One run's measured numbers: the PR side and the `dev` side of the table.
pub(crate) struct Report {
    pub(crate) revision: String,
    pub(crate) wasm_raw: Option<u64>,
    pub(crate) wasm_gzip: Option<u64>,
    /// Kernel name (`simulation::advance`) to the observed instruction count.
    pub(crate) instructions: BTreeMap<String, Option<u64>>,
    /// Scenario name to tick p50/p95 in nanoseconds.
    pub(crate) ticks: BTreeMap<String, Timing>,
    pub(crate) pressure: Option<Pressure>,
}

/// Nearest-rank percentile of recorded samples.
pub(crate) fn percentile(samples: &[u64], percent: u64) -> Option<u64> {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (sorted.len().checked_mul(percent as usize)?)
        .div_ceil(100)
        .max(1);
    sorted.get(rank - 1).copied()
}

fn file_size(dir: &Path, name: &str) -> Option<u64> {
    let metadata = fs::metadata(dir.join(name)).ok()?;
    metadata.is_file().then_some(metadata.len())
}

fn timing(samples: &[u64]) -> Timing {
    Timing {
        p50: percentile(samples, 50),
        p95: percentile(samples, 95),
    }
}

fn ticks(report: &Value) -> BTreeMap<String, Timing> {
    let mut ticks = BTreeMap::new();
    let Some(workloads) = report.get("workloads").and_then(Value::as_array) else {
        return ticks;
    };
    for workload in workloads {
        let Some(name) = workload.get("scenario").and_then(Value::as_str) else {
            continue;
        };
        let samples: Vec<u64> = workload
            .get("tick_duration_ns")
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        ticks.insert(name.to_owned(), timing(&samples));
    }
    ticks
}

/// The `instructions::…` comparison rows: the pinned kernels of
/// `make perf-instructions`, with their observed counts (absent when the
/// run itself recorded them as unavailable).
fn instructions(report: &Value) -> BTreeMap<String, Option<u64>> {
    let mut instructions = BTreeMap::new();
    let Some(comparisons) = report.get("comparisons").and_then(Value::as_array) else {
        return instructions;
    };
    for comparison in comparisons {
        let Some(metric) = comparison.get("metric").and_then(Value::as_str) else {
            continue;
        };
        let Some(kernel) = metric.strip_prefix("instructions::") else {
            continue;
        };
        // The benchmark module is itself named `instructions`; strip it too.
        let kernel = kernel.trim_start_matches("instructions::");
        instructions.insert(
            kernel.to_owned(),
            comparison.get("observed").and_then(Value::as_u64),
        );
    }
    instructions
}

fn pressure(dir: &Path) -> Option<Pressure> {
    let report: Value = serde_json::from_slice(&fs::read(dir.join("pressure.json")).ok()?).ok()?;
    let latency: Vec<u64> = report
        .get("snapshot_latency_us")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    Some(Pressure {
        snapshot_latency_us: timing(&latency),
        max_response_backlog: report.get("max_response_backlog").and_then(Value::as_u64),
        missed_offer_deadlines: report.get("missed_offer_deadlines").and_then(Value::as_u64),
    })
}

fn revision(report: &Value) -> String {
    report
        .get("revision")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned()
}

/// The artifact of one CI run: `ci.json` is the core report; the optimized
/// WASM files and the pressure report are optional siblings.
pub(crate) fn read(dir: &Path) -> Result<Report, String> {
    let bytes =
        fs::read(dir.join("ci.json")).map_err(|_| format!("{} has no ci.json", dir.display()))?;
    let report: Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", dir.display()))?;
    Ok(Report {
        revision: revision(&report),
        wasm_raw: file_size(dir, "optimized.wasm"),
        wasm_gzip: file_size(dir, "optimized.wasm.gz"),
        instructions: instructions(&report),
        ticks: ticks(&report),
        pressure: pressure(dir),
    })
}

/// The WASM size rule the pull request's own gate applies: the baseline in
/// `baselines/perf/wasm.json` of the checked-out revision and the cap the
/// gate's comparison threshold derives from it.
#[derive(Debug, PartialEq)]
pub(crate) struct WasmRule {
    pub(crate) baseline: u64,
    pub(crate) cap: u64,
}

pub(crate) fn wasm_rule_at(path: &Path) -> Option<WasmRule> {
    #[derive(Deserialize)]
    struct Baseline {
        optimized_gzip_bytes: u64,
    }
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice::<Baseline>(&bytes)
        .ok()
        .map(|baseline| WasmRule {
            baseline: baseline.optimized_gzip_bytes,
            // The largest size the gate still passes: one below the smallest
            // value it calls a regression.
            cap: crate::perf::regression_floor(
                baseline.optimized_gzip_bytes,
                crate::perf_size::JUMP_PERCENT,
            ) - 1,
        })
}
