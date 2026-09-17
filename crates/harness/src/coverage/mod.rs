//! Native production-line coverage policy over cargo-llvm-cov LCOV output.
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

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

    fn meets(self, floor: usize) -> bool {
        self.measurable > 0 && self.covered * 100 >= floor * self.measurable
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
    verdict: &'static str,
}

fn expected_sources(root: &Path) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "crates",
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err("cannot inventory first-party Rust sources".into());
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|item| !item.is_empty())
        .map(|item| String::from_utf8_lossy(item).to_string())
        .filter(|name| {
            name.starts_with("crates/")
                && name.contains("/src/")
                && name.ends_with(".rs")
                && !name.ends_with("/tests.rs")
                && !name.starts_with("crates/client/src/")
                && !name.starts_with("crates/rendering/src/")
        })
        .collect())
}

fn source_cutoff(path: &Path) -> Result<usize, Box<dyn Error>> {
    let source = fs::read_to_string(path)?;
    Ok(source
        .lines()
        .position(|line| line.trim() == "#[cfg(test)]")
        .map_or(usize::MAX, |index| index + 1))
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

fn parse(root: &Path, data: &str, expected: &BTreeSet<String>) -> Result<Report, Box<dyn Error>> {
    let root = root.canonicalize()?;
    let mut cutoffs = BTreeMap::<PathBuf, usize>::new();
    let mut current = None::<(String, usize)>;
    let mut seen_sources = BTreeSet::new();
    let mut seen_lines = BTreeSet::new();
    let mut counts = [Count::default(); 4];
    for line in data.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            let path = Path::new(path).canonicalize()?;
            let relative = path.strip_prefix(&root)?;
            let name = relative.to_str().ok_or("non-UTF-8 coverage path")?;
            current = if expected.contains(name) {
                let cutoff = *cutoffs.entry(path.clone()).or_insert(source_cutoff(&path)?);
                seen_sources.insert(name.to_owned());
                Some((name.to_owned(), cutoff))
            } else {
                None
            };
        } else if let Some(rest) = line.strip_prefix("DA:") {
            if let Some((name, cutoff)) = &current {
                let (number, after) = rest.split_once(',').ok_or("invalid LCOV DA record")?;
                let hits = after.split(',').next().ok_or("invalid LCOV hit count")?;
                let number: usize = number.parse()?;
                let hits: u64 = hits.parse()?;
                if number < *cutoff {
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
    let missing_sources: Vec<_> = expected.difference(&seen_sources).cloned().collect();
    let passed = counts[0].meets(85)
        && counts[1].meets(90)
        && counts[2].meets(90)
        && counts[3].meets(90)
        && missing_sources.is_empty();
    Ok(Report {
        version: 1,
        overall: counts[0],
        policy_reports: counts[1],
        protocol: counts[2],
        asset_parsers: counts[3],
        missing_sources,
        verdict: if passed { "PASS" } else { "REGRESSION" },
    })
}

pub fn check(path: &Path) -> Result<(), Box<dyn Error>> {
    let root = Path::new(".");
    let expected = expected_sources(root)?;
    let report = parse(root, &fs::read_to_string(path)?, &expected)?;
    fs::create_dir_all("reports/coverage")?;
    fs::write(
        "reports/coverage/summary.json",
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.verdict != "PASS" {
        return Err("native production-line coverage is below a required floor".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_groups_or_test_only_coverage_cannot_pass() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let file = temp.path().join("crates/protocol/src/lib.rs");
        fs::create_dir_all(file.parent().expect("parent")).expect("directory");
        fs::write(
            &file,
            "pub fn run() {}\n#[cfg(test)]\nmod tests { fn covered() {} }\n",
        )
        .expect("source");
        let lcov = format!("SF:{}\nDA:1,0\nDA:3,1\nend_of_record\n", file.display());
        let report = parse(
            temp.path(),
            &lcov,
            &BTreeSet::from([
                "crates/protocol/src/lib.rs".to_owned(),
                "crates/core/src/lib.rs".to_owned(),
            ]),
        )
        .expect("report");
        assert_eq!(report.protocol.measurable, 1);
        assert_eq!(report.protocol.covered, 0);
        assert_eq!(report.missing_sources, ["crates/core/src/lib.rs"]);
        assert_eq!(report.verdict, "REGRESSION");
    }
}
