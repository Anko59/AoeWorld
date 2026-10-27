//! Parse exact Gungraun JSON samples and compare reviewed microbench baselines.
use crate::perf::{Comparison, compare};
use aoe_scenario::{LATE_GAME, SMOKE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, error::Error, fs, path::Path};

#[derive(Deserialize, Serialize)]
struct Baseline {
    version: u16,
    toolchain: String,
    gungraun: String,
    valgrind: String,
    allow_aslr: bool,
    smoke_workload_hash: String,
    late_game_workload_hash: String,
    instructions: BTreeMap<String, u64>,
    allocation_bytes: BTreeMap<String, u64>,
    allocation_count: BTreeMap<String, u64>,
}

#[derive(Clone, Copy)]
struct Metrics {
    instructions: u64,
    allocation_bytes: u64,
    allocation_count: u64,
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

fn sample(line: &str) -> Result<(String, Metrics), Box<dyn Error>> {
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
    let dhat = profiles
        .iter()
        .find(|profile| profile["tool"] == "DHAT")
        .ok_or("DHAT sample missing")?;
    Ok((
        name,
        Metrics {
            instructions: metric(callgrind, "Callgrind", "Ir", "Both")?,
            allocation_bytes: metric(dhat, "Dhat", "TotalBytes", "Both")?,
            allocation_count: metric(dhat, "Dhat", "TotalBlocks", "Both")?,
        },
    ))
}

fn samples(path: &Path) -> Result<BTreeMap<String, Metrics>, Box<dyn Error>> {
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
) -> Result<BTreeMap<String, Metrics>, Box<dyn Error>> {
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

fn proposal(observed: &BTreeMap<String, Metrics>) -> Result<Baseline, Box<dyn Error>> {
    if observed.len() != 5 {
        return Err("initial proposal requires five benchmark cases".into());
    }
    Ok(Baseline {
        version: 1,
        toolchain: "rustc 1.99.1".into(),
        gungraun: "0.19.4".into(),
        valgrind: "3.19.0".into(),
        allow_aslr: true,
        smoke_workload_hash: SMOKE.workload_hash(),
        late_game_workload_hash: LATE_GAME.workload_hash(),
        instructions: observed
            .iter()
            .map(|(name, value)| (name.clone(), value.instructions))
            .collect(),
        allocation_bytes: observed
            .iter()
            .map(|(name, value)| (name.clone(), value.allocation_bytes))
            .collect(),
        allocation_count: observed
            .iter()
            .map(|(name, value)| (name.clone(), value.allocation_count))
            .collect(),
    })
}

pub fn propose() -> Result<(), Box<dyn Error>> {
    propose_at(
        Path::new("reports/perf/simulation.ndjson"),
        Path::new("reports/perf/protocol.ndjson"),
        Path::new("reports/perf/assets.ndjson"),
        Path::new("reports/perf/micro-proposal.json"),
    )
}

fn propose_at(
    simulation: &Path,
    protocol: &Path,
    assets: &Path,
    output: &Path,
) -> Result<(), Box<dyn Error>> {
    let observed = observed_samples_at(simulation, protocol, assets)?;
    let proposal = proposal(&observed)?;
    fs::create_dir_all(output.parent().ok_or("proposal has no parent")?)?;
    fs::write(output, serde_json::to_vec_pretty(&proposal)?)?;
    println!("review reports/perf/micro-proposal.json; baseline was not changed");
    Ok(())
}

fn compare_observed(
    baseline: Baseline,
    observed: BTreeMap<String, Metrics>,
) -> Result<Vec<Comparison>, Box<dyn Error>> {
    if baseline.instructions.len() != 5
        || baseline.version != 1
        || baseline.toolchain != "rustc 1.99.1"
        || baseline.gungraun != "0.19.4"
        || baseline.valgrind != "3.19.0"
        || !baseline.allow_aslr
        || baseline.smoke_workload_hash != SMOKE.workload_hash()
        || baseline.late_game_workload_hash != LATE_GAME.workload_hash()
    {
        return Err("microbenchmark baseline identity is incompatible".into());
    }
    let expected: Vec<_> = baseline.instructions.keys().collect();
    if expected != observed.keys().collect::<Vec<_>>()
        || expected != baseline.allocation_bytes.keys().collect::<Vec<_>>()
        || expected != baseline.allocation_count.keys().collect::<Vec<_>>()
    {
        return Err("microbenchmark case set differs from baseline".into());
    }
    let mut results = Vec::new();
    for (name, counts) in observed {
        results.push(compare(
            &format!("instructions::{name}"),
            Some(counts.instructions),
            baseline.instructions.get(&name).copied(),
        ));
        results.push(compare(
            &format!("allocation_bytes::{name}"),
            Some(counts.allocation_bytes),
            baseline.allocation_bytes.get(&name).copied(),
        ));
        results.push(compare(
            &format!("allocation_count::{name}"),
            Some(counts.allocation_count),
            baseline.allocation_count.get(&name).copied(),
        ));
    }
    Ok(results)
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
