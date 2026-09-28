//! Native production-line coverage policy over cargo-llvm-cov LCOV output.
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
    if report.verdict != "PASS" {
        return Err("native production-line coverage is below a required floor".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, process::Command};

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
            &SourceInventory::from([
                ("crates/protocol/src/lib.rs".to_owned(), BTreeSet::from([1])),
                ("crates/core/src/lib.rs".to_owned(), BTreeSet::from([1])),
            ]),
        )
        .expect("report");
        assert_eq!(report.protocol.measurable, 1);
        assert_eq!(report.protocol.covered, 0);
        assert_eq!(report.missing_sources, ["crates/core/src/lib.rs"]);
        assert_eq!(report.verdict, "REGRESSION");
    }

    #[test]
    fn coverage_gate_inventories_sources_and_rejects_duplicate_or_missing_records() {
        let temp = tempfile::tempdir().expect("repository");
        let root = temp.path();
        let status = Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .status()
            .expect("git init");
        assert!(status.success());
        let names = [
            "crates/harness/src/qa.rs",
            "crates/protocol/src/lib.rs",
            "crates/assets/src/drs.rs",
            "crates/core/src/lib.rs",
        ];
        fs::create_dir_all(root.join("crates/harness/src")).expect("harness directory");
        fs::write(root.join("crates/harness/src/main.rs"), "mod qa;\n").expect("harness root");
        fs::create_dir_all(root.join("crates/assets/src")).expect("assets directory");
        fs::write(root.join("crates/assets/src/lib.rs"), "pub mod drs;\n").expect("assets root");
        let mut lcov = String::new();
        for name in names {
            let source = root.join(name);
            fs::create_dir_all(source.parent().expect("parent")).expect("source directory");
            fs::write(&source, "pub fn covered() {}\n").expect("source");
            lcov.push_str(&format!("SF:{}\nDA:1,1\nend_of_record\n", source.display()));
        }
        let excluded = root.join("crates/client/src/lib.rs");
        fs::create_dir_all(excluded.parent().expect("parent")).expect("client directory");
        fs::write(excluded, "pub fn browser() {}\n").expect("client source");
        assert_eq!(
            expected_sources(root).expect("inventory").len(),
            names.len()
        );
        let report_path = root.join("native.lcov");
        fs::write(&report_path, &lcov).expect("LCOV");
        check_at(root, &report_path).expect("complete report");
        let summary: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("reports/coverage/summary.json")).expect("summary"),
        )
        .expect("JSON");
        assert_eq!(summary["verdict"], "PASS");
        let duplicate = lcov.replace("DA:1,1\n", "DA:1,1\nDA:1,1\n");
        fs::write(&report_path, duplicate).expect("duplicate LCOV");
        assert!(check_at(root, &report_path).is_err());
        fs::write(
            &report_path,
            lcov.lines().skip(3).collect::<Vec<_>>().join("\n"),
        )
        .expect("missing source");
        assert!(check_at(root, &report_path).is_err());
    }

    #[test]
    fn missing_production_functions_and_macros_remain_a_regression() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let root = temp.path();
        let status = Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .status()
            .expect("git init");
        assert!(status.success());
        let write_source = |name: &str, source: &str| {
            let path = root.join(name);
            fs::create_dir_all(path.parent().expect("source parent")).expect("source directory");
            fs::write(path, source).expect("source");
        };
        write_source(
            "crates/demo/src/lib.rs",
            "mod function;\nmod macros;\nmod types;\n",
        );
        write_source("crates/demo/src/function.rs", "pub fn uncovered() {}\n");
        write_source(
            "crates/demo/src/macros.rs",
            "macro_rules! uncovered_macro { () => { 1 }; }\n",
        );
        write_source("crates/demo/src/types.rs", "pub struct TypeOnly;\n");
        let expected = expected_sources(root).expect("inventory");
        assert!(expected.contains_key("crates/demo/src/function.rs"));
        assert!(expected.contains_key("crates/demo/src/macros.rs"));
        assert!(!expected.contains_key("crates/demo/src/types.rs"));
        let function = root.join("crates/demo/src/function.rs");
        let lcov = format!("SF:{}\nDA:1,1\nend_of_record\n", function.display());
        let report = parse(root, &lcov, &expected).expect("report");
        assert_eq!(
            report.missing_sources,
            ["crates/demo/src/macros.rs".to_owned()]
        );
        assert_eq!(report.verdict, "REGRESSION");
    }
}
