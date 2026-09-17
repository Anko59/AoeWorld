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

fn observed_samples_at(
    simulation: &Path,
    protocol: &Path,
) -> Result<BTreeMap<String, Metrics>, Box<dyn Error>> {
    let mut observed = samples(simulation)?;
    for (name, value) in samples(protocol)? {
        if observed.insert(name, value).is_some() {
            return Err("duplicate cross-crate microbenchmark".into());
        }
    }
    Ok(observed)
}

fn proposal(observed: &BTreeMap<String, Metrics>) -> Result<Baseline, Box<dyn Error>> {
    if observed.len() != 3 {
        return Err("initial proposal requires three benchmark cases".into());
    }
    Ok(Baseline {
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
    })
}

pub fn propose() -> Result<(), Box<dyn Error>> {
    propose_at(
        Path::new("reports/perf/simulation.ndjson"),
        Path::new("reports/perf/protocol.ndjson"),
        Path::new("reports/perf/micro-proposal.json"),
    )
}

fn propose_at(simulation: &Path, protocol: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let observed = observed_samples_at(simulation, protocol)?;
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
    if baseline.version != 1
        || baseline.toolchain != "rustc 1.93.1"
        || baseline.gungraun != "0.19.4"
        || baseline.valgrind != "3.19.0"
        || !baseline.allow_aslr
        || baseline.smoke_workload_hash != SMOKE.workload_hash()
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
    )
}

fn comparisons_at(
    baseline: &Path,
    simulation: &Path,
    protocol: &Path,
) -> Result<Vec<Comparison>, Box<dyn Error>> {
    let baseline: Baseline = serde_json::from_slice(&fs::read(baseline)?)?;
    compare_observed(baseline, observed_samples_at(simulation, protocol)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perf::Verdict;

    fn line(name: &str, instructions: u64) -> String {
        serde_json::json!({
            "version": "6",
            "module_path": name,
            "profiles": [
                {
                    "tool": "Callgrind",
                    "summaries": {"total": {"summary": {
                        "Callgrind": {"Ir": {"metrics": {"Both": [{"Int": instructions}]}}}
                    }}}
                },
                {
                    "tool": "DHAT",
                    "summaries": {"total": {"summary": {
                        "Dhat": {
                            "TotalBytes": {"metrics": {"Both": [{"Int": 80}]}},
                            "TotalBlocks": {"metrics": {"Both": [{"Int": 2}]}}
                        }
                    }}}
                }
            ]
        })
        .to_string()
    }

    #[test]
    fn missing_samples_cannot_pass() {
        assert!(sample("{\"version\":\"6\",\"module_path\":\"x\"}").is_err());
        assert!(
            sample(&line("kernel", 100).replace("\"version\":\"6\"", "\"version\":\"5\"")).is_err()
        );
        let (name, counts) = sample(&line("kernel", 100)).expect("sample");
        assert_eq!(name, "kernel");
        assert_eq!(
            (
                counts.instructions,
                counts.allocation_bytes,
                counts.allocation_count
            ),
            (100, 80, 2)
        );

        let temp = tempfile::NamedTempFile::new().expect("file");
        assert!(samples(temp.path()).is_err());
        fs::write(
            temp.path(),
            format!("{}\n{}\n", line("same", 100), line("same", 100)),
        )
        .expect("samples");
        assert!(samples(temp.path()).is_err());
    }

    #[test]
    fn baseline_identity_case_set_and_regressions_are_checked() {
        let mut observed = BTreeMap::new();
        for name in ["a", "b", "c"] {
            observed.insert(
                name.to_owned(),
                Metrics {
                    instructions: 100,
                    allocation_bytes: 80,
                    allocation_count: 2,
                },
            );
        }
        assert!(proposal(&BTreeMap::new()).is_err());
        let baseline = proposal(&observed).expect("proposal");
        assert!(compare_observed(baseline, BTreeMap::new()).is_err());

        let mut baseline = proposal(&observed).expect("proposal");
        baseline.toolchain = "different".to_owned();
        assert!(compare_observed(baseline, observed.clone()).is_err());

        let baseline = proposal(&observed).expect("proposal");
        observed.get_mut("a").expect("case").instructions = 106;
        let results = compare_observed(baseline, observed).expect("comparison");
        assert_eq!(results.len(), 9);
        assert_eq!(results[0].verdict, Verdict::Regression);
        assert!(
            results[1..]
                .iter()
                .all(|item| item.verdict == Verdict::Pass)
        );
    }

    #[test]
    fn proposal_reads_both_suite_files_without_changing_baseline() {
        let temp = tempfile::tempdir().expect("directory");
        let simulation = temp.path().join("simulation.ndjson");
        let protocol = temp.path().join("protocol.ndjson");
        let baseline = temp.path().join("baseline.json");
        let proposed = temp.path().join("proposal/micro.json");
        fs::write(
            &simulation,
            format!("{}\n{}\n", line("a", 100), line("b", 100)),
        )
        .expect("simulation samples");
        fs::write(&protocol, format!("{}\n", line("c", 100))).expect("protocol samples");
        assert_eq!(
            observed_samples_at(&simulation, &protocol)
                .expect("samples")
                .len(),
            3
        );
        let original = proposal(&observed_samples_at(&simulation, &protocol).expect("samples"))
            .expect("baseline");
        let original_bytes = serde_json::to_vec(&original).expect("JSON");
        fs::write(&baseline, &original_bytes).expect("baseline");
        propose_at(&simulation, &protocol, &proposed).expect("proposal");
        assert_eq!(fs::read(&baseline).expect("baseline"), original_bytes);
        assert_eq!(
            comparisons_at(&baseline, &simulation, &protocol)
                .expect("comparison")
                .len(),
            9
        );
        assert!(proposed.is_file());
        fs::write(&protocol, format!("{}\n", line("a", 100))).expect("duplicate");
        assert!(observed_samples_at(&simulation, &protocol).is_err());
    }
}
