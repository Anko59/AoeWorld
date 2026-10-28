//! Parse exact Gungraun/Callgrind JSON samples and gate instruction counts.
//!
//! Instructions are deterministic, so the gate is tight and semi-permissive:
//! above [`WARN_PERCENT`] over the committed baseline a row warns, above
//! [`FAIL_PERCENT`] it fails, and a row more than [`CREEP_PERCENT`] over the
//! fixed `instruction_reference` fails whatever the baseline says, so many
//! small accepted steps cannot add up. A drop past [`FAIL_PERCENT`] asks for a
//! baseline refresh (`make perf-baseline-propose`, reviewed in a commit).
use crate::perf::{Comparison, Verdict, compare_at, floor_at, over};
use aoe_scenario::{LATE_GAME, SMOKE, TARGET_DISTRIBUTED};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, error::Error, fs, path::Path};

pub const WARN_PERCENT: u8 = 1;
pub const FAIL_PERCENT: u8 = 3;
pub const CREEP_PERCENT: u8 = 10;

/// Simulation (advance, viewport, late game, smoke and distributed ticks),
/// protocol (1) and assets (1).
const CASES: usize = 7;

#[derive(Deserialize, Serialize)]
struct Baseline {
    version: u16,
    toolchain: String,
    gungraun: String,
    valgrind: String,
    allow_aslr: bool,
    smoke_workload_hash: String,
    late_game_workload_hash: String,
    target_distributed_workload_hash: String,
    instructions: BTreeMap<String, u64>,
    /// Fixed anchor for the creep guard. Refreshing `instructions` never moves
    /// it; only a deliberate, reviewed edit does.
    instruction_reference: BTreeMap<String, u64>,
}

fn metric(profile: &Value, tool: &str, name: &str, side: &str) -> Result<u64, Box<dyn Error>> {
    let metrics = &profile["summaries"]["total"]["summary"][tool][name]["metrics"];
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

fn sample(line: &str) -> Result<(String, u64), Box<dyn Error>> {
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
    Ok((name, metric(callgrind, "Callgrind", "Ir", "Both")?))
}

fn samples(path: &Path) -> Result<BTreeMap<String, u64>, Box<dyn Error>> {
    let content = fs::read_to_string(path)?;
    let mut result = BTreeMap::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let (name, counts) = sample(line)?;
        if result.insert(name, counts).is_some() {
            return Err("duplicate microbenchmark".into());
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
        target_distributed_workload_hash: TARGET_DISTRIBUTED.workload_hash(),
        instructions: observed.clone(),
        instruction_reference: observed
            .iter()
            .map(|(name, value)| (name.clone(), reference.get(name).copied().unwrap_or(*value)))
            .collect(),
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

fn compare_observed(
    baseline: Baseline,
    observed: BTreeMap<String, u64>,
) -> Result<Vec<Comparison>, Box<dyn Error>> {
    if baseline.instructions.len() != CASES
        || baseline.version != 1
        || baseline.toolchain != "rustc 1.99.1"
        || baseline.gungraun != "0.19.4"
        || baseline.valgrind != "3.19.0"
        || !baseline.allow_aslr
        || baseline.smoke_workload_hash != SMOKE.workload_hash()
        || baseline.late_game_workload_hash != LATE_GAME.workload_hash()
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
    Ok(observed
        .into_iter()
        .map(|(name, count)| {
            judge(
                &format!("instructions::{name}"),
                count,
                baseline.instructions[&name],
                baseline.instruction_reference[&name],
            )
        })
        .collect())
}

/// One instruction row: fail above +3% of `baseline` or +10% of `reference`,
/// warn above +1%, ask for a refresh below -3%.
fn judge(metric: &str, observed: u64, baseline: u64, reference: u64) -> Comparison {
    let mut row = compare_at(metric, Some(observed), Some(baseline), FAIL_PERCENT);
    let delta =
        |from: u64| (i128::from(observed) - i128::from(from)) as f64 * 100.0 / from.max(1) as f64;
    if row.verdict == Verdict::Regression {
        row.note = Some(format!(
            "FAIL {:+.1}% over baseline {baseline} (cap +{FAIL_PERCENT}%)",
            delta(baseline)
        ));
    } else if over(observed, reference, CREEP_PERCENT) {
        row.verdict = Verdict::Regression;
        row.note = Some(format!(
            "FAIL creep {:+.1}% over reference {reference} (cap {})",
            delta(reference),
            floor_at(reference, CREEP_PERCENT) - 1
        ));
    } else if over(observed, baseline, WARN_PERCENT) {
        row.note = Some(format!(
            "warning {:+.1}% over baseline {baseline} (fails above +{FAIL_PERCENT}%)",
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

pub fn comparisons() -> Result<Vec<Comparison>, Box<dyn Error>> {
    comparisons_at(
        Path::new("baselines/perf/micro.json"),
        Path::new("reports/perf/simulation.ndjson"),
        Path::new("reports/perf/protocol.ndjson"),
        Path::new("reports/perf/assets.ndjson"),
    )
}

fn comparisons_at(
    baseline: &Path,
    simulation: &Path,
    protocol: &Path,
    assets: &Path,
) -> Result<Vec<Comparison>, Box<dyn Error>> {
    let baseline: Baseline = serde_json::from_slice(&fs::read(baseline)?)?;
    compare_observed(baseline, observed_samples_at(simulation, protocol, assets)?)
}

#[cfg(test)]
mod tests;
