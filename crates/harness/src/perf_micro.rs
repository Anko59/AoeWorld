//! Parse exact Gungraun/Callgrind JSON samples and gate instruction counts.
//!
//! Instructions are deterministic, so the gate is tight and semi-permissive:
//! above [`WARN_PERCENT`] over the committed baseline a row warns, above
//! [`FAIL_PERCENT`] it fails, and a row more than [`CREEP_PERCENT`] over the
//! fixed `instruction_reference` fails whatever the baseline says, so many
//! small accepted steps cannot add up. A drop past [`FAIL_PERCENT`] asks for a
//! baseline refresh (`make perf-baseline-propose`, reviewed in a commit).
//!
//! On pull requests a row also fails more than [`FAIL_PERCENT`] above the
//! latest passing `dev` measurement ([`dev`]), so a gain merged on `dev`
//! cannot be spent by later changes while the baseline is stale.
//!
//! A kernel run on worker threads (Callgrind `--separate-threads=yes`) gives
//! two rows: its main thread (`<kernel>_main`), the serial path a tick waits
//! on, gated as above; and its total over every thread, which work moved to
//! the workers does not shrink, failing only above [`PARALLEL_TOTAL_PERCENT`].
use crate::perf::{Comparison, Verdict, compare_at, floor_at, over};
use aoe_scenario::{LATE_GAME, LATE_GAME_X4, SMOKE, TARGET_DISTRIBUTED};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, error::Error, fs, path::Path};

pub mod critical;
pub mod dev;
use dev::DevReference;

pub const WARN_PERCENT: u8 = 1;
pub const FAIL_PERCENT: u8 = 3;
pub const CREEP_PERCENT: u8 = 10;
/// The total of a kernel that also reports its main thread.
pub const PARALLEL_TOTAL_PERCENT: u8 = 10;
const MAIN_THREAD: &str = "_main";

/// Simulation (advance, viewport, late game and four-times late game, each
/// total and main thread, smoke and distributed ticks), protocol (1) and assets (1).
const CASES: usize = 10;

#[derive(Clone, Deserialize, Serialize)]
struct Baseline {
    version: u16,
    toolchain: String,
    gungraun: String,
    valgrind: String,
    allow_aslr: bool,
    smoke_workload_hash: String,
    late_game_workload_hash: String,
    late_game_x4_workload_hash: String,
    target_distributed_workload_hash: String,
    instructions: BTreeMap<String, u64>,
    /// Fixed anchor for the creep guard. Refreshing `instructions` never moves
    /// it; only a deliberate, reviewed edit does.
    instruction_reference: BTreeMap<String, u64>,
    /// Rows whose raised `instructions` a reviewed commit accepted (owner
    /// 11-24: removing a cap may cost instructions). While the dev run still
    /// counts what it counted when the increase was accepted (within the fail
    /// margin), the row is gated against its baseline, not the dev run. Once
    /// the increase lands, dev counts more and the entry has no effect (unless
    /// a later gain takes dev back to that count); a baseline refresh drops it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    accepted_increase: BTreeMap<String, Accepted>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Accepted {
    reason: String,
    /// The dev run's count the increase was accepted over.
    dev: u64,
}

fn metric(profile: &Value, tool: &str, name: &str, side: &str) -> Result<u64, Box<dyn Error>> {
    metric_in(&profile["summaries"]["total"]["summary"], tool, name, side)
}

fn metric_in(summary: &Value, tool: &str, name: &str, side: &str) -> Result<u64, Box<dyn Error>> {
    let metrics = &summary[tool][name]["metrics"];
    let node = if metrics[side].is_null() {
        &metrics["Left"]
    } else {
        &metrics[side]
    };
    let value = node
        .as_array()
        .and_then(|array| array.first())
        .unwrap_or(node);
    value["Int"]
        .as_u64()
        .ok_or_else(|| format!("missing {tool} {name} sample").into())
}

/// Callgrind's thread 1, when the run was split by thread: the main thread.
fn main_thread(profile: &Value) -> Result<Option<u64>, Box<dyn Error>> {
    let Some(parts) = profile["summaries"]["parts"].as_array() else {
        return Ok(None);
    };
    if parts.len() < 2 {
        return Ok(None);
    }
    let thread = |part: &Value| {
        let details = &part["details"];
        let info = if details["Left"].is_null() {
            &details["Both"][0]
        } else {
            &details["Left"]
        };
        info["thread"].as_u64()
    };
    let main = parts
        .iter()
        .find(|part| thread(part) == Some(1))
        .ok_or("threads were split but thread 1 is missing")?;
    Ok(Some(metric_in(
        &main["metrics_summary"],
        "Callgrind",
        "Ir",
        "Both",
    )?))
}

/// The kernel's total and, for a run split by thread, its main thread.
fn sample(line: &str) -> Result<Vec<(String, u64)>, Box<dyn Error>> {
    let item: Value = serde_json::from_str(line)?;
    if item["version"] != "6" {
        return Err("unexpected Gungraun JSON version".into());
    }
    let name = item["module_path"]
        .as_str()
        .ok_or("benchmark name missing")?
        .to_owned();
    let profiles = item["profiles"].as_array().ok_or("profiles missing")?;
    let callgrind = profiles
        .iter()
        .find(|profile| profile["tool"] == "Callgrind")
        .ok_or("Callgrind sample missing")?;
    let mut rows = vec![(name.clone(), metric(callgrind, "Callgrind", "Ir", "Both")?)];
    if let Some(main) = main_thread(callgrind)? {
        rows.push((format!("{name}{MAIN_THREAD}"), main));
    }
    Ok(rows)
}

fn samples(path: &Path) -> Result<BTreeMap<String, u64>, Box<dyn Error>> {
    let content = fs::read_to_string(path)?;
    let mut result = BTreeMap::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        for (name, counts) in sample(line)? {
            if result.insert(name, counts).is_some() {
                return Err("duplicate microbenchmark".into());
            }
        }
    }
    if result.is_empty() {
        return Err("microbenchmark returned no samples".into());
    }
    Ok(result)
}

fn observed_samples_at(
    simulation: &Path,
    protocol: &Path,
    assets: &Path,
) -> Result<BTreeMap<String, u64>, Box<dyn Error>> {
    let mut observed = samples(simulation)?;
    for suite in [protocol, assets] {
        for (name, value) in samples(suite)? {
            if observed.insert(name, value).is_some() {
                return Err("duplicate cross-crate microbenchmark".into());
            }
        }
    }
    Ok(observed)
}

/// `reference` is the committed anchor to keep; a case without one starts at
/// its observed count.
fn proposal(
    observed: &BTreeMap<String, u64>,
    reference: &BTreeMap<String, u64>,
) -> Result<Baseline, Box<dyn Error>> {
    if observed.len() != CASES {
        return Err(format!("initial proposal requires {CASES} benchmark cases").into());
    }
    Ok(Baseline {
        version: 1,
        toolchain: "rustc 1.99.1".into(),
        gungraun: "0.19.4".into(),
        valgrind: "3.19.0".into(),
        allow_aslr: true,
        smoke_workload_hash: SMOKE.workload_hash(),
        late_game_workload_hash: LATE_GAME.workload_hash(),
        late_game_x4_workload_hash: LATE_GAME_X4.workload_hash(),
        target_distributed_workload_hash: TARGET_DISTRIBUTED.workload_hash(),
        instructions: observed.clone(),
        instruction_reference: observed
            .iter()
            .map(|(name, value)| (name.clone(), reference.get(name).copied().unwrap_or(*value)))
            .collect(),
        accepted_increase: BTreeMap::new(),
    })
}

pub fn propose() -> Result<(), Box<dyn Error>> {
    propose_at(
        Path::new("reports/perf/simulation.ndjson"),
        Path::new("reports/perf/protocol.ndjson"),
        Path::new("reports/perf/assets.ndjson"),
        Path::new("baselines/perf/micro.json"),
        Path::new("reports/perf/micro-proposal.json"),
    )
}

fn propose_at(
    simulation: &Path,
    protocol: &Path,
    assets: &Path,
    baseline: &Path,
    output: &Path,
) -> Result<(), Box<dyn Error>> {
    let observed = observed_samples_at(simulation, protocol, assets)?;
    let reference = fs::read(baseline)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Baseline>(&bytes).ok())
        .map(|baseline| baseline.instruction_reference)
        .unwrap_or_default();
    let proposal = proposal(&observed, &reference)?;
    fs::create_dir_all(output.parent().ok_or("proposal has no parent")?)?;
    fs::write(output, serde_json::to_vec_pretty(&proposal)?)?;
    println!("review reports/perf/micro-proposal.json; baseline was not changed");
    Ok(())
}

#[cfg(test)]
fn compare_observed(
    baseline: Baseline,
    observed: BTreeMap<String, u64>,
) -> Result<Vec<Comparison>, Box<dyn Error>> {
    compare_with_dev(baseline, observed, None)
}

fn compare_with_dev(
    baseline: Baseline,
    observed: BTreeMap<String, u64>,
    dev: Option<&DevReference>,
) -> Result<Vec<Comparison>, Box<dyn Error>> {
    if baseline.instructions.len() != CASES
        || baseline.version != 1
        || baseline.toolchain != "rustc 1.99.1"
        || baseline.gungraun != "0.19.4"
        || baseline.valgrind != "3.19.0"
        || !baseline.allow_aslr
        || baseline.smoke_workload_hash != SMOKE.workload_hash()
        || baseline.late_game_workload_hash != LATE_GAME.workload_hash()
        || baseline.late_game_x4_workload_hash != LATE_GAME_X4.workload_hash()
        || baseline.target_distributed_workload_hash != TARGET_DISTRIBUTED.workload_hash()
    {
        return Err("microbenchmark baseline identity is incompatible".into());
    }
    let expected: Vec<_> = baseline.instructions.keys().collect();
    if expected != observed.keys().collect::<Vec<_>>()
        || expected != baseline.instruction_reference.keys().collect::<Vec<_>>()
    {
        return Err("microbenchmark case set differs from baseline".into());
    }
    let split: Vec<_> = observed
        .keys()
        .filter_map(|name| name.strip_suffix(MAIN_THREAD))
        .map(str::to_owned)
        .collect();
    Ok(observed
        .into_iter()
        .map(|(name, count)| {
            let fail = if split.contains(&name) {
                PARALLEL_TOTAL_PERCENT
            } else {
                FAIL_PERCENT
            };
            let accepted = baseline.accepted_increase.get(&name).map(|a| a.dev);
            let latest = dev
                .and_then(|dev| {
                    let kernel = name.strip_prefix("instructions::").unwrap_or(&name);
                    Some((dev.instructions.get(kernel).copied()?, dev.run))
                })
                .filter(|&(count, _)| accepted.is_none_or(|from| over(count, from, fail)));
            judge(
                &format!("instructions::{name}"),
                count,
                baseline.instructions[&name],
                baseline.instruction_reference[&name],
                latest,
                fail,
            )
        })
        .collect())
}

/// One instruction row: fail above +`fail`% of `baseline`, +10% of
/// `reference` or +`fail`% of the latest passing dev run (`latest`: its count
/// and run id), warn above +1% of the baseline, ask for a refresh below -3%.
fn judge(
    metric: &str,
    observed: u64,
    baseline: u64,
    reference: u64,
    latest: Option<(u64, u64)>,
    fail: u8,
) -> Comparison {
    let mut row = compare_at(metric, Some(observed), Some(baseline), fail);
    let delta =
        |from: u64| (i128::from(observed) - i128::from(from)) as f64 * 100.0 / from.max(1) as f64;
    if row.verdict == Verdict::Regression {
        row.note = Some(format!(
            "FAIL {:+.1}% over baseline {baseline} (cap +{fail}%)",
            delta(baseline)
        ));
    } else if over(observed, reference, CREEP_PERCENT) {
        row.verdict = Verdict::Regression;
        row.note = Some(format!(
            "FAIL creep {:+.1}% over reference {reference} (cap {})",
            delta(reference),
            floor_at(reference, CREEP_PERCENT) - 1
        ));
    } else if let Some((dev, run)) = latest
        && over(observed, dev, fail)
    {
        row.verdict = Verdict::Regression;
        row.note = Some(format!(
            "FAIL {:+.1}% over latest passing dev run {run} ({dev}; cap +{fail}%)",
            delta(dev)
        ));
    } else if over(observed, baseline, WARN_PERCENT) {
        row.note = Some(format!(
            "warning {:+.1}% over baseline {baseline} (fails above +{fail}%)",
            delta(baseline)
        ));
    } else if u128::from(observed) * 100 < u128::from(baseline) * u128::from(100 - FAIL_PERCENT) {
        row.note = Some(format!(
            "improved {:+.1}%: refresh the baseline with make perf-baseline-propose",
            delta(baseline)
        ));
    }
    row
}

/// Informational critical-path rows (`<kernel>_critical`) of the kernels run
/// on worker threads, from the per-thread profiles Gungraun kept. They carry
/// no baseline and never fail; a kernel whose profiles are missing says so.
fn critical_rows(simulation: &Path) -> Vec<Comparison> {
    let Ok(content) = fs::read_to_string(simulation) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(item) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(module) = item["module_path"].as_str() else {
            continue;
        };
        let split = item["profiles"]
            .as_array()
            .and_then(|profiles| profiles.iter().find(|p| p["tool"] == "Callgrind"))
            .and_then(|p| p["summaries"]["parts"].as_array())
            .is_some_and(|parts| parts.len() > 1);
        if !split {
            continue;
        }
        let root = Path::new(item["project_root"].as_str().unwrap_or("."));
        let result = critical::locate(root, module)
            .ok_or_else(|| "the per-thread Callgrind profiles are gone".to_owned())
            .and_then(|dir| critical::read_dir(&dir));
        let (observed, note) = match result {
            Ok(critical) => (Some(critical.path()), critical.note()),
            Err(reason) => (
                None,
                format!("informational: critical path unavailable: {reason}"),
            ),
        };
        rows.push(Comparison {
            metric: format!("instructions::{module}_critical"),
            observed,
            baseline: None,
            threshold_percent: 0,
            verdict: Verdict::Pass,
            note: Some(note),
        });
    }
    rows
}

/// The rows and the notes that say which references they were checked against.
pub fn comparisons() -> Result<(Vec<Comparison>, Vec<String>), Box<dyn Error>> {
    comparisons_at(
        Path::new("baselines/perf/micro.json"),
        Path::new("reports/perf/simulation.ndjson"),
        Path::new("reports/perf/protocol.ndjson"),
        Path::new("reports/perf/assets.ndjson"),
        Path::new(dev::PATH),
    )
}

fn comparisons_at(
    baseline: &Path,
    simulation: &Path,
    protocol: &Path,
    assets: &Path,
    dev_file: &Path,
) -> Result<(Vec<Comparison>, Vec<String>), Box<dyn Error>> {
    let baseline: Baseline = serde_json::from_slice(&fs::read(baseline)?)?;
    let observed = observed_samples_at(simulation, protocol, assets)?;
    let (reference, note) = match dev::load(dev_file) {
        Ok(reference) => {
            let note = format!(
                "instructions also gated against the latest passing dev run {} ({}): fail above +{FAIL_PERCENT}%",
                reference.run, reference.revision
            );
            (Some(reference), note)
        }
        Err(reason) => (
            None,
            format!(
                "dev reference unavailable ({reason}): instructions gated against the committed baseline only"
            ),
        ),
    };
    let mut rows = compare_with_dev(baseline, observed, reference.as_ref())?;
    rows.extend(critical_rows(simulation));
    Ok((rows, vec![note]))
}

#[cfg(test)]
mod tests;
