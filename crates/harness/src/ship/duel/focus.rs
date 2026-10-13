//! Dockerized checks of tester-selected packages/specs, with named local results.
use super::{browser, output, runner, tree};
use crate::{
    process::{Cancellation, CaptureExit, capture_command, capture_command_retaining_stdout},
    ship::git,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader},
    path::Path,
    process::Command,
    time::Duration,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct CheckReport {
    pub(crate) passed: bool,
    #[serde(default)]
    pub(crate) build_failed: bool,
    #[serde(default)]
    pub(crate) unavailable: Option<String>,
    pub(crate) tests: Vec<String>,
    pub(crate) failed: Vec<String>,
    pub(crate) untouched: Vec<String>,
    /// Wasm suites whose tests the runner did not name: their entry in `tests`/`failed`
    /// is only an exit status.
    #[serde(default)]
    pub(crate) exit_only: Vec<String>,
    #[serde(default)]
    pub(crate) locations: BTreeMap<String, (String, usize)>,
}

pub(crate) use super::scope::Scope;

pub(crate) fn spec_filter(path: &str) -> String {
    let escaped: String = path
        .chars()
        .flat_map(|c| {
            if ".*+?^$[](){}|\\".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect();
    format!("(^|/){escaped}$")
}

pub(crate) fn rust_results(output: &str) -> CheckReport {
    let mut result = CheckReport::default();
    for line in output.lines() {
        add_rust_result(&mut result, line);
    }
    result.tests.sort();
    result.tests.dedup();
    result.failed.sort();
    result.failed.dedup();
    result
}
fn add_rust_result(result: &mut CheckReport, line: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    if v["type"] != "test" {
        return;
    }
    let Some(name) = v["name"].as_str() else {
        return;
    };
    match v["event"].as_str() {
        Some("ok") => result.tests.push(name.into()),
        Some("failed") => {
            result.tests.push(name.into());
            result.failed.push(name.into());
        }
        _ => (),
    }
}

pub(crate) fn pre_existing(current: &CheckReport, baseline: &CheckReport) -> bool {
    !current.passed
        && !current.build_failed
        && !baseline.build_failed
        && current.unavailable.is_none()
        && baseline.unavailable.is_none()
        && !current.failed.is_empty()
        && current
            .failed
            .iter()
            .all(|name| current.untouched.contains(name) && baseline.failed.contains(name))
}

pub(crate) fn execute(root: &Path) -> git::Result<()> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let base = std::env::var("HARNESS_DUEL_BASE").map_err(|_| "HARNESS_DUEL_BASE is required")?;
    let revision = std::env::var("HARNESS_DUEL_REVISION")
        .ok()
        .filter(|s| !s.is_empty());
    let cache = root.join(".cache/duel");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut scope: Scope = if revision.is_some() {
        serde_json::from_slice(&fs::read(cache.join("scope.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
    } else {
        let paths = tree::since(&root, &base)?
            .into_iter()
            .filter(|p| crate::agents::is_test_path(&p.to_ascii_lowercase()))
            .collect::<Vec<_>>();
        let scope = Scope::from_paths(&root, &paths)?;
        fs::write(
            cache.join("scope.json"),
            serde_json::to_vec(&scope).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        scope
    };
    let temporary;
    let check_root = if let Some(revision) = revision.as_deref() {
        temporary = super::baseline::export(&root, revision)?;
        temporary.path()
    } else {
        &root
    };
    // The original scope is persisted for the separate browser adapter.
    scope.specs.clear();
    scope.wasm.clear();
    let result = run_scope(check_root, &scope, &cache);
    let mut report = match result {
        Ok(report) => report,
        Err(error) => {
            let report = CheckReport {
                failed: vec![format!("build/check unavailable: {error}")],
                build_failed: error.starts_with("build failed:"),
                unavailable: (!error.starts_with("build failed:")).then_some(error.clone()),
                ..Default::default()
            };
            save(&cache, &report)?;
            return Err(error);
        }
    };
    if revision.is_none() && !report.build_failed && !report.failed.is_empty() {
        report.untouched = super::baseline::untouched(&root, &base, &scope, &report.failed)?;
    }
    save(&cache, &report)?;
    if report.passed {
        Ok(())
    } else {
        Err(format!(
            "focused tests failed: {}",
            report.failed.join(", ")
        ))
    }
}
pub(super) fn save(cache: &Path, report: &CheckReport) -> git::Result<()> {
    fs::write(
        cache.join("check.json"),
        serde_json::to_vec(report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "Focused {}: {}",
        if report.passed { "PASS" } else { "FAIL" },
        if report.failed.is_empty() {
            report.tests.join(", ")
        } else {
            report.failed.join(", ")
        }
    );
    Ok(())
}
pub(super) fn run_scope(root: &Path, scope: &Scope, cache: &Path) -> git::Result<CheckReport> {
    let mut report = CheckReport {
        passed: true,
        ..Default::default()
    };
    if !scope.packages.is_empty() {
        let log = output::run_directory(cache)
            .map_err(|e| e.to_string())?
            .join("nextest.jsonl");
        println!("Retained nextest output: {}", log.display());
        let mut file = output::JsonOutput::create(&log, "nextest").map_err(|e| e.to_string())?;
        let mut command = Command::new("cargo");
        command
            .args(scope.cargo_args())
            .current_dir(root)
            .env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1");
        if root != cache.parent().and_then(Path::parent).unwrap_or(root) {
            command.env("CARGO_TARGET_DIR", cache.join("baseline-target"));
        }
        let captured = capture_command_retaining_stdout(
            command,
            Duration::from_secs(3600),
            &Cancellation::default(),
            &mut file,
        );
        if let CaptureExit::Monitor(error) = &captured.exit {
            return Err(format!("nextest JSON retention unavailable: {error}"));
        }
        // Stream the retained JSON so failures cannot disappear from a bounded output tail.
        let mut compile_failed = false;
        for line in BufReader::new(fs::File::open(log).map_err(|e| e.to_string())?).lines() {
            let line = line.map_err(|e| e.to_string())?;
            compile_failed |= package_compile_error(&line, &scope.packages);
            let result = rust_results(&line);
            report.tests.extend(result.tests);
            report.failed.extend(result.failed);
        }
        match captured.exit {
            CaptureExit::Success if !report.tests.is_empty() => (),
            CaptureExit::Success => return Err("focused Rust check ran no tests".into()),
            CaptureExit::Failed(_) if !report.failed.is_empty() => report.passed = false,
            CaptureExit::Failed(Some(_))
                if compile_failed
                    || runner::tail(&captured)
                        .lines()
                        .any(|line| package_compile_error(line, &scope.packages)) =>
            {
                report.passed = false;
                report.build_failed = true;
                report.failed.push(format!(
                    "package build failed: {}\n{}",
                    scope.packages.join(", "),
                    runner::tail(&captured)
                ));
            }
            _ => {
                return Err(format!(
                    "package check unavailable: {:?}\n{}",
                    captured.exit,
                    runner::tail(&captured)
                ));
            }
        }
    }
    if !scope.specs.is_empty() {
        let report_path = output::run_directory(cache)
            .map_err(|e| e.to_string())?
            .join("playwright.json");
        println!("Retained Playwright output: {}", report_path.display());
        browser::dependencies(root, cache)?;
        browser_build(root)?;
        let previous = std::env::current_dir().map_err(|e| e.to_string())?;
        std::env::set_current_dir(root).map_err(|e| e.to_string())?;
        let filters: Vec<_> = scope
            .specs
            .iter()
            .map(|p| spec_filter(p.strip_prefix("browser/").unwrap_or(p)))
            .collect();
        let checked = crate::e2e::run_focused(&filters, &report_path).map_err(|e| e.to_string());
        std::env::set_current_dir(previous).map_err(|e| e.to_string())?;
        let browser = browser_results(&report_path)?;
        report.passed &= browser.failed.is_empty();
        report.locations.extend(browser.locations);
        report.tests.extend(browser.tests);
        report.failed.extend(browser.failed);
        if let Err(error) = checked {
            if report.failed.is_empty() {
                return Err(error);
            }
            report.passed = false;
        }
    }
    if !scope.wasm.is_empty() {
        let wasm = super::wasm::check(root, &scope.wasm, cache)?;
        report.passed &= wasm.passed;
        report.build_failed |= wasm.build_failed;
        report.tests.extend(wasm.tests);
        report.failed.extend(wasm.failed);
        report.exit_only.extend(wasm.exit_only);
    }
    report.tests.sort();
    report.tests.dedup();
    report.failed.sort();
    report.failed.dedup();
    Ok(report)
}

fn package_compile_error(line: &str, packages: &[String]) -> bool {
    if let Some(message) = line.trim().strip_prefix("error: could not compile `") {
        return message
            .split_once('`')
            .is_some_and(|(package, _)| packages.iter().any(|p| p == package));
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return false;
    };
    value["reason"] == "compiler-message"
        && value["message"]["level"] == "error"
        && value["package_id"].as_str().is_some_and(|id| {
            let name = id.rsplit('#').next().unwrap_or(id);
            let name = name.split(['@', ' ']).next().unwrap_or(name);
            packages.iter().any(|p| p == name)
        })
}

pub(super) fn browser_results(path: &Path) -> git::Result<CheckReport> {
    let value: serde_json::Value =
        serde_json::from_slice(&output::read(path, "Playwright")?).map_err(|e| e.to_string())?;
    fn walk(v: &serde_json::Value, parents: &str, report: &mut CheckReport) {
        if let Some(specs) = v["specs"].as_array() {
            for spec in specs {
                let file = spec["file"].as_str().unwrap_or("unknown");
                let title = spec["title"].as_str().unwrap_or("unknown");
                if let Some(tests) = spec["tests"].as_array() {
                    for test in tests {
                        let name = format!(
                            "{file}::{parents}{title} [{}]",
                            test["projectName"].as_str().unwrap_or("unknown")
                        );
                        if test["status"] != "skipped" {
                            report.tests.push(name.clone());
                        }
                        report.locations.insert(
                            name.clone(),
                            (
                                if file.starts_with("tests/") {
                                    format!("browser/{file}")
                                } else {
                                    format!("browser/tests/{file}")
                                },
                                spec["line"].as_u64().unwrap_or(0) as usize,
                            ),
                        );
                        if !matches!(test["status"].as_str(), Some("expected" | "skipped")) {
                            report.failed.push(name);
                        }
                    }
                }
            }
        }
        if let Some(suites) = v["suites"].as_array() {
            for suite in suites {
                let title = suite["title"].as_str().unwrap_or("");
                let parents = if title == suite["file"].as_str().unwrap_or("") {
                    parents.to_owned()
                } else {
                    format!("{parents}{title}::")
                };
                walk(suite, &parents, report);
            }
        }
    }
    let mut report = CheckReport::default();
    walk(&value, "", &mut report);
    if value["errors"].as_array().is_some_and(|a| !a.is_empty()) || report.locations.is_empty() {
        return Err("Playwright errors or no focused tests".into());
    }
    Ok(report)
}

fn browser_build(root: &Path) -> git::Result<()> {
    for (program, args) in [
        (
            "cargo",
            vec!["build", "--locked", "--release", "-p", "aoe-server"],
        ),
        (
            "cargo",
            vec![
                "build",
                "--locked",
                "--profile",
                "wasm-release",
                "--target",
                "wasm32-unknown-unknown",
                "-p",
                "aoe-client",
            ],
        ),
        (
            "wasm-bindgen",
            vec![
                "--target",
                "web",
                "--out-dir",
                "web/pkg",
                "--out-name",
                "aoe_client",
                "target/wasm32-unknown-unknown/wasm-release/aoe_client.wasm",
            ],
        ),
    ] {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(root)
            .env_remove("CARGO_TARGET_DIR");
        let captured =
            capture_command(command, Duration::from_secs(3600), &Cancellation::default());
        if !matches!(captured.exit, CaptureExit::Success) {
            return Err(format!(
                "{}browser prerequisite {program}: {:?}\n{}",
                if matches!(captured.exit, CaptureExit::Failed(Some(_))) {
                    "build failed: "
                } else {
                    "unavailable: "
                },
                captured.exit,
                runner::tail(&captured)
            ));
        }
    }
    Ok(())
}

pub(super) fn classify_specs(root: &Path, base: &str, report: &mut CheckReport) -> git::Result<()> {
    for (name, (path, line)) in &report.locations {
        if !report.failed.contains(name) {
            continue;
        }
        let current = fs::read_to_string(root.join(path)).map_err(|e| e.to_string())?;
        if let Ok(old) = git::git(root, &["show", &format!("{base}:{path}")])
            && super::specs::unchanged(&current, &old, *line)
        {
            report.untouched.push(name.clone());
        }
    }
    Ok(())
}
