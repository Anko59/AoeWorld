//! Native production-line coverage report over cargo-llvm-cov LCOV output.
mod inventory;

use inventory::{SourceInventory, expected_sources};
use serde::Serialize;
use std::{collections::BTreeSet, error::Error, fs, path::Path};

#[derive(Clone, Copy, Default, Serialize)]
struct Count {
    covered: usize,
    measurable: usize,
}

impl Count {
    fn record(&mut self, hits: u64) {
        self.measurable += 1;
        self.covered += usize::from(hits > 0);
    }
}

#[derive(Serialize)]
struct Report {
    version: u16,
    overall: Count,
    policy_reports: Count,
    protocol: Count,
    asset_parsers: Count,
    missing_sources: Vec<String>,
}

fn classify(path: &str) -> (bool, bool, bool) {
    let policy = matches!(
        path,
        "crates/harness/src/architecture.rs"
            | "crates/harness/src/gates.rs"
            | "crates/harness/src/perf_micro.rs"
            | "crates/harness/src/perf_size.rs"
            | "crates/harness/src/policy.rs"
            | "crates/harness/src/qa.rs"
            | "crates/harness/src/repo_policy.rs"
    );
    let protocol = path == "crates/protocol/src/lib.rs";
    let asset_parser = matches!(
        path,
        "crates/assets/src/drs.rs"
            | "crates/assets/src/lib.rs"
            | "crates/assets/src/palette.rs"
            | "crates/assets/src/slp.rs"
    );
    (policy, protocol, asset_parser)
}

fn parse(root: &Path, data: &str, expected: &SourceInventory) -> Result<Report, Box<dyn Error>> {
    let root = root.canonicalize()?;
    let mut current = None::<String>;
    let mut seen_sources = BTreeSet::new();
    let mut seen_lines = BTreeSet::new();
    let mut counts = [Count::default(); 4];
    for line in data.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            let path = Path::new(path).canonicalize()?;
            let relative = path.strip_prefix(&root)?;
            let name = relative.to_str().ok_or("non-UTF-8 coverage path")?;
            current = if expected.contains_key(name) {
                Some(name.to_owned())
            } else {
                None
            };
        } else if let Some(rest) = line.strip_prefix("DA:") {
            if let Some(name) = &current {
                let (number, after) = rest.split_once(',').ok_or("invalid LCOV DA record")?;
                let hits = after.split(',').next().ok_or("invalid LCOV hit count")?;
                let number: usize = number.parse()?;
                let hits: u64 = hits.parse()?;
                if expected[name].contains(&number) {
                    seen_sources.insert(name.clone());
                    if !seen_lines.insert((name.clone(), number)) {
                        return Err("duplicate LCOV line".into());
                    }
                    counts[0].record(hits);
                    let (policy, protocol, asset_parser) = classify(name);
                    if policy {
                        counts[1].record(hits);
                    }
                    if protocol {
                        counts[2].record(hits);
                    }
                    if asset_parser {
                        counts[3].record(hits);
                    }
                }
            }
        } else if line == "end_of_record" {
            current = None;
        }
    }
    let missing_sources: Vec<_> = expected
        .keys()
        .filter(|name| !seen_sources.contains(*name))
        .cloned()
        .collect();
    Ok(Report {
        version: 1,
        overall: counts[0],
        policy_reports: counts[1],
        protocol: counts[2],
        asset_parsers: counts[3],
        missing_sources,
    })
}

pub fn check(path: &Path) -> Result<(), Box<dyn Error>> {
    check_at(Path::new("."), path)
}

fn check_at(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
    let expected = expected_sources(root)?;
    let report = parse(root, &fs::read_to_string(path)?, &expected)?;
    fs::create_dir_all(root.join("reports/coverage"))?;
    fs::write(
        root.join("reports/coverage/summary.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests;
