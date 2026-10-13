//! Measured values, read from reports that already exist for a commit. Nothing
//! here builds, tests or measures: a missing or stale report is simply absent.
use super::super::{evidence, git};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(in crate::ship) struct Perf {
    pub(in crate::ship) p95_tick_ns: u64,
    pub(in crate::ship) entities: u64,
    pub(in crate::ship) bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(in crate::ship) struct Measured {
    pub(in crate::ship) coverage_percent: Option<f64>,
    pub(in crate::ship) wasm_bytes: Option<u64>,
    pub(in crate::ship) wasm_gzip_bytes: Option<u64>,
    pub(in crate::ship) native_bytes: Option<u64>,
    pub(in crate::ship) perf: Option<Perf>,
    pub(in crate::ship) gate_seconds: Option<f64>,
}

impl Measured {
    /// Values here win; `other` fills what is missing.
    pub(in crate::ship) fn or(self, other: Self) -> Self {
        Self {
            coverage_percent: self.coverage_percent.or(other.coverage_percent),
            wasm_bytes: self.wasm_bytes.or(other.wasm_bytes),
            wasm_gzip_bytes: self.wasm_gzip_bytes.or(other.wasm_gzip_bytes),
            native_bytes: self.native_bytes.or(other.native_bytes),
            perf: self.perf.or(other.perf),
            gate_seconds: self.gate_seconds.or(other.gate_seconds),
        }
    }
}

/// Reports in `root` describe the checked-out commit only, and only when they
/// were written after it was committed.
fn committed_at(root: &Path, sha: &str) -> Option<SystemTime> {
    if git::git(root, &["rev-parse", "HEAD"]).ok()? != sha {
        return None;
    }
    let seconds: u64 = git::git(root, &["log", "-1", "--format=%ct", sha])
        .ok()?
        .parse()
        .ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

fn fresh_bytes(path: &Path, since: SystemTime) -> Option<Vec<u8>> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    (modified >= since).then(|| fs::read(path).ok()).flatten()
}

fn fresh_size(path: &Path, since: SystemTime) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    (meta.is_file() && meta.modified().ok()? >= since).then_some(meta.len())
}

fn coverage(root: &Path, since: SystemTime) -> Option<f64> {
    let bytes = fresh_bytes(&root.join("reports/coverage/summary.json"), since)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let covered = value.pointer("/overall/covered")?.as_u64()?;
    let measurable = value.pointer("/overall/measurable")?.as_u64()?;
    (measurable > 0).then(|| covered as f64 * 100.0 / measurable as f64)
}

/// The optimized bundle from the perf run, else the largest built `web/pkg` module.
fn wasm_bytes(root: &Path, since: SystemTime) -> Option<u64> {
    if let Some(size) = fresh_size(&root.join("reports/perf/optimized.wasm"), since) {
        return Some(size);
    }
    fs::read_dir(root.join("web/pkg"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "wasm"))
        .filter_map(|path| fresh_size(&path, since))
        .max()
}

/// Nearest-rank 95th percentile.
fn p95(samples: &[u64]) -> Option<u64> {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (sorted.len() * 95).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}

fn perf(root: &Path, sha: &str, since: SystemTime) -> Option<Perf> {
    let bytes = fresh_bytes(&root.join("reports/perf/smoke.json"), since)?;
    let report: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if report.get("revision")?.as_str()? != sha || report.get("dirty")?.as_bool()? {
        return None;
    }
    let workloads = report.get("workloads")?.as_array()?;
    let workload = workloads
        .iter()
        .find(|item| item.get("scenario").and_then(|name| name.as_str()) == Some("smoke"))
        .or(workloads.first())?;
    let ticks: Vec<u64> = workload
        .get("tick_duration_ns")?
        .as_array()?
        .iter()
        .filter_map(|tick| tick.as_u64())
        .collect();
    Some(Perf {
        p95_tick_ns: p95(&ticks)?,
        entities: workload.get("total_entities")?.as_u64()?,
        bytes: workload.get("encoded_bytes")?.as_u64()?,
    })
}

/// Total gate seconds of the ship evidence recorded for exactly this commit.
fn gate_seconds(root: &Path, sha: &str) -> Option<f64> {
    let recorded = evidence::read(root, sha).ok()??;
    Some(
        recorded
            .gates
            .iter()
            .map(|gate| f64::from(gate.seconds))
            .sum(),
    )
}

pub(in crate::ship) fn read(root: &Path, sha: &str) -> Measured {
    let mut measured = Measured {
        gate_seconds: gate_seconds(root, sha),
        ..Measured::default()
    };
    if let Some(since) = committed_at(root, sha) {
        measured.coverage_percent = coverage(root, since);
        measured.wasm_bytes = wasm_bytes(root, since);
        measured.wasm_gzip_bytes = fresh_size(&root.join("reports/perf/optimized.wasm.gz"), since);
        measured.native_bytes = fresh_size(&root.join("target/release/aoe-server"), since);
        measured.perf = perf(root, sha, since);
    }
    measured
}
