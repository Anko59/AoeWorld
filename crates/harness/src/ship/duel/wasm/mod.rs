//! Focused check of packages that `test-wasm` runs: their wasm-bindgen-test suites, named like nextest.
use super::{focus::CheckReport, output, runner};
use crate::{
    process::CaptureExit,
    ship::git,
    wasm_test::{self, SuiteRun},
};
use std::{fs, path::Path};

pub(super) fn check(root: &Path, wasm: &[String], cache: &Path) -> git::Result<CheckReport> {
    let target = (root != cache.parent().and_then(Path::parent).unwrap_or(root))
        .then(|| cache.join("baseline-target"));
    let runs = wasm_test::run_focused(root, wasm, target.as_deref())
        .map_err(|e| format!("wasm check unavailable: {e}"))?;
    let log = output::run_directory(cache)
        .map_err(|e| e.to_string())?
        .join("wasm.log");
    let text: Vec<_> = runs.iter().map(|r| r.output.as_str()).collect();
    fs::write(&log, text.join("\n")).map_err(|e| e.to_string())?;
    println!("Retained wasm output: {}", log.display());
    results(&runs)
}

/// Fold suite runs into one report. A run that ended without a verdict is unavailable.
pub(super) fn results(runs: &[SuiteRun]) -> git::Result<CheckReport> {
    let mut report = CheckReport {
        passed: true,
        ..Default::default()
    };
    for run in runs {
        let passed = match run.exit {
            CaptureExit::Success => true,
            CaptureExit::Failed(Some(_)) => false,
            ref other => {
                return Err(format!(
                    "wasm check unavailable: {} {other:?}\n{}",
                    run.package,
                    tail(&run.output)
                ));
            }
        };
        let binary = if run.binary == "lib" {
            run.package.to_owned()
        } else {
            format!("{}::{}", run.package, run.binary)
        };
        let mut named = 0;
        for line in run.output.lines() {
            let Some(rest) = line.trim().strip_prefix("test ") else {
                continue;
            };
            let Some((name, status)) = rest.rsplit_once(" ... ") else {
                continue;
            };
            let crate_name = if run.binary == "lib" {
                run.package.replace('-', "_")
            } else {
                run.binary.to_owned()
            };
            let name = name
                .strip_prefix(&format!("{crate_name}::"))
                .unwrap_or(name);
            let name = format!("{binary}${name}");
            match status.trim() {
                "ok" => report.tests.push(name),
                // wasm-bindgen-test prints FAIL; libtest-style runners print FAILED.
                "FAIL" | "FAILED" => {
                    report.tests.push(name.clone());
                    report.failed.push(name);
                }
                _ => continue,
            }
            named += 1;
        }
        let failed_named = report
            .failed
            .iter()
            .any(|n| n.starts_with(&format!("{binary}$")));
        if passed && !failed_named {
            if named == 0 {
                // The runner printed no test lines: exit status is the only evidence.
                let name = exit_only_name(run.package);
                report.tests.push(name.clone());
                report.exit_only.push(name);
            }
            continue;
        }
        report.passed = false;
        if failed_named {
            continue;
        }
        if run.output.contains("wasm-bindgen-test-runner")
            && ["No such file", "not found", "could not execute"]
                .iter()
                .any(|m| run.output.contains(m))
        {
            return Err(format!(
                "wasm check unavailable: {}\n{}",
                run.package,
                tail(&run.output)
            ));
        }
        if run.output.lines().any(|l| {
            let l = l.trim_start();
            l.starts_with("error: could not compile") || l.starts_with("error[E")
        }) {
            report.build_failed = true;
            report.failed.push(format!(
                "wasm package build failed: {}\n{}",
                run.package,
                tail(&run.output)
            ));
        } else {
            let name = exit_only_name(run.package);
            report.failed.push(name.clone());
            report.exit_only.push(name);
        }
    }
    Ok(report)
}

pub(super) fn exit_only_name(package: &str) -> String {
    format!("{package}::wasm-suite")
}

fn tail(output: &str) -> String {
    let lines: Vec<_> = output.lines().rev().take(40).collect();
    runner::bounded(&lines.into_iter().rev().collect::<Vec<_>>().join("\n"))
}
