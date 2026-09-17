//! Parse exact Gungraun JSON samples and compare reviewed microbench baselines.
use crate::perf::{Comparison, compare};
use aoe_scenario::SMOKE;
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

fn observed_samples() -> Result<BTreeMap<String, Metrics>, Box<dyn Error>> {
    let mut observed = samples(Path::new("reports/perf/simulation.ndjson"))?;
    for (name, value) in samples(Path::new("reports/perf/protocol.ndjson"))? {
        if observed.insert(name, value).is_some() {
            return Err("duplicate cross-crate microbenchmark".into());
        }
    }
    Ok(observed)
}

pub fn propose() -> Result<(), Box<dyn Error>> {
    let observed = observed_samples()?;
    if observed.len() != 3 {
        return Err("initial proposal requires three benchmark cases".into());
    }
    let proposal = Baseline {
        version: 1,
        toolchain: "rustc 1.93.1".into(),
        gungraun: "0.19.4".into(),
        valgrind: "3.19.0".into(),
        allow_aslr: true,
        smoke_workload_hash: SMOKE.workload_hash(),
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
    };
    fs::create_dir_all("reports/perf")?;
    fs::write(
        "reports/perf/micro-proposal.json",
        serde_json::to_vec_pretty(&proposal)?,
    )?;
    println!("review reports/perf/micro-proposal.json; baseline was not changed");
    Ok(())
}

pub fn comparisons() -> Result<Vec<Comparison>, Box<dyn Error>> {
    let baseline: Baseline = serde_json::from_slice(&fs::read("baselines/perf/micro.json")?)?;
    if baseline.version != 1
        || baseline.toolchain != "rustc 1.93.1"
        || baseline.gungraun != "0.19.4"
        || baseline.valgrind != "3.19.0"
        || !baseline.allow_aslr
        || baseline.smoke_workload_hash != SMOKE.workload_hash()
    {
        return Err("microbenchmark baseline identity is incompatible".into());
    }
    let observed = observed_samples()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_samples_cannot_pass() {
        assert!(sample("{\"version\":\"6\",\"module_path\":\"x\"}").is_err());
    }
}
