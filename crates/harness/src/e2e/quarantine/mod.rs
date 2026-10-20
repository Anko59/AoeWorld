//! Rust-owned selective retries; Playwright itself keeps retries disabled.
use crate::{
    gates::flaky,
    process::{self, Cancellation, CaptureExit},
};
use serde_json::Value;
use std::{error::Error, fs, path::Path, process::Command, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
mod evidence;

pub(super) const REPORTER: &str = "--reporter=./quarantine-reporter.ts,json";

#[derive(Debug, PartialEq)]
struct Case {
    key: String,
    file: String,
    line: u64,
    title: String,
    project: String,
    status: String,
    passed: bool,
}

fn escape(text: &str) -> String {
    let mut escaped = String::new();
    for c in text.chars() {
        if "\\^$.*+?()[]{}|".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

impl Case {
    fn selector(&self) -> [String; 3] {
        [
            format!("/^{}$/:{}", escape(&self.file), self.line),
            format!("--project={}", self.project),
            format!("--grep=/{}$/", escape(&self.title)),
        ]
    }

    fn name(&self) -> String {
        format!("{} [{}] {}", self.key, self.project, self.title)
    }
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Playwright report missing {key}").into())
}

fn cases(value: &Value) -> Result<Vec<Case>> {
    let mut evidence = evidence::Evidence::new(value)?;
    if !value
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
    {
        return Err("Playwright report has missing/global errors".into());
    }
    value
        .get("suites")
        .and_then(Value::as_array)
        .ok_or("missing Playwright suites")?;
    let root = Path::new(string(
        value.get("config").ok_or("missing Playwright config")?,
        "rootDir",
    )?);
    if !root.is_absolute()
        || root
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err("invalid Playwright rootDir".into());
    }
    let mut cases = Vec::new();
    collect(value, &[], root, &mut cases, &mut evidence)?;
    evidence.finish()?;
    let stats = value
        .get("stats")
        .ok_or("Playwright report missing stats")?;
    for (field, status) in [
        ("expected", "expected"),
        ("unexpected", "unexpected"),
        ("flaky", "flaky"),
        ("skipped", "skipped"),
    ] {
        if stats.get(field).and_then(Value::as_u64)
            != Some(cases.iter().filter(|case| case.status == status).count() as u64)
        {
            return Err(format!("incomplete Playwright report: {field}").into());
        }
    }
    Ok(cases)
}

fn collect(
    suite: &Value,
    parents: &[String],
    root: &Path,
    cases: &mut Vec<Case>,
    evidence: &mut evidence::Evidence<'_>,
) -> Result<()> {
    if let Some(specs) = suite.get("specs").and_then(Value::as_array) {
        for spec in specs {
            let file = string(spec, "file")?;
            let key = file
                .strip_suffix(".spec.ts")
                .ok_or("invalid Playwright spec file")?;
            if file.starts_with('/') || file.split('/').any(|part| part == "..") {
                return Err("invalid Playwright spec path".into());
            }
            let line = spec
                .get("line")
                .and_then(Value::as_u64)
                .filter(|line| *line > 0)
                .ok_or("missing spec line")?;
            let mut titles = parents.to_vec();
            titles.push(string(spec, "title")?.to_owned());
            for test in spec
                .get("tests")
                .and_then(Value::as_array)
                .ok_or("missing Playwright tests")?
            {
                let status = string(test, "status")?;
                if !matches!(status, "expected" | "unexpected" | "skipped") {
                    return Err("unexpected Playwright retry/status".into());
                }
                let results = test
                    .get("results")
                    .and_then(Value::as_array)
                    .ok_or("missing test results")?;
                let [result] = results.as_slice() else {
                    return Err("test must have exactly one attempt".into());
                };
                let actual = string(result, "status")?;
                let expected = string(test, "expectedStatus")?;
                if !matches!(actual, "passed" | "failed" | "timedOut" | "skipped")
                    || (status == "unexpected" && (expected != "passed" || actual != "failed"))
                    || (status == "expected" && actual != expected)
                    || (status == "skipped" && (actual != "skipped" || expected != "skipped"))
                {
                    return Err("inconsistent or interrupted Playwright test result".into());
                }
                evidence.case(test, status == "unexpected")?;
                cases.push(Case {
                    key: key.to_owned(),
                    file: root.join(file).to_string_lossy().into_owned(),
                    line,
                    title: titles.join(" "),
                    project: string(test, "projectName")?.to_owned(),
                    status: status.to_owned(),
                    passed: actual == "passed",
                });
            }
        }
    }
    for child in suite
        .get("suites")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let mut titles = parents.to_vec();
        let title = string(child, "title")?;
        // The outer file suite is not part of the spec's title path.
        if !title.ends_with(".spec.ts") && !title.is_empty() {
            titles.push(title.to_owned());
        }
        collect(child, &titles, root, cases, evidence)?;
    }
    Ok(())
}

fn failures(value: &Value) -> Result<Vec<Case>> {
    Ok(cases(value)?
        .into_iter()
        .filter(|case| case.status == "unexpected")
        .collect())
}

fn check_exit(success: bool, value: &Value) -> Result<()> {
    if success != (value["config"]["metadata"]["aoeQuarantine"]["status"] == "passed") {
        return Err("Playwright exit disagrees with full-run status".into());
    }
    Ok(())
}

fn retry_args(original: &[String], case: &Case) -> Result<Vec<String>> {
    let separator = original
        .iter()
        .position(|arg| arg == REPORTER)
        .ok_or("missing JSON reporter")?;
    let mut args = original[..=separator].to_vec();
    // Initial spec filters and sharding apply once; select exactly this failed case.
    args.extend(case.selector());
    Ok(args)
}

fn attempt(
    args: &[String],
    report: &Path,
    deadline: Duration,
    cancellation: &Cancellation,
) -> Result<(bool, Value)> {
    let _ = fs::remove_file(report);
    let mut output = crate::ship::JsonOutput::create(report, "Playwright")?;
    let mut command = Command::new("docker");
    command.args(args);
    let captured =
        process::capture_command_retaining_stdout(command, deadline, cancellation, &mut output);
    let success = match captured.exit {
        CaptureExit::Success => true,
        CaptureExit::Failed(Some(1)) => false,
        _ => {
            process::report_captured("docker", captured, deadline)?;
            return Err("Playwright unavailable".into());
        }
    };
    if !success {
        let _ = process::report_captured("docker", captured, deadline);
    }
    drop(output);
    let value = serde_json::from_slice(&fs::read(report)?)?;
    Ok((success, value))
}

pub(super) fn run(
    args: &[String],
    report: &Path,
    journal: &Path,
    deadline: Duration,
    cancellation: &Cancellation,
    enable_quarantine: bool,
) -> Result<()> {
    let policy = enable_quarantine
        .then(|| flaky::Policy::load(Path::new(".")))
        .transpose()?;
    let mut journal = flaky::Journal::create(journal)?;
    resolve(
        policy.as_ref(),
        &mut journal,
        args,
        report,
        |args, report| attempt(args, report, deadline, cancellation),
    )
}

fn resolve(
    policy: Option<&flaky::Policy>,
    journal: &mut flaky::Journal,
    args: &[String],
    report: &Path,
    mut run_attempt: impl FnMut(&[String], &Path) -> Result<(bool, Value)>,
) -> Result<()> {
    let (success, value) = run_attempt(args, report)?;
    let failed = failures(&value)?;
    check_exit(success, &value)?;
    if success {
        if !failed.is_empty() {
            return Err("successful Playwright exit with failed tests".into());
        }
        println!("test-e2e: Playwright passed; report: {}", report.display());
        return Ok(());
    }
    if failed.is_empty() {
        return Err("Playwright failed without complete named failures".into());
    }
    // The duel's browser_exit recognizes this completed assertion-failure line.
    // Keep runner/cancellation errors distinct from ordinary red evidence.
    if policy.is_none() {
        return Err("focused Playwright check unavailable: Failed(Some(1))".into());
    }
    let mut unresolved = Vec::new();
    for (index, case) in failed.iter().enumerate() {
        let mut attempt_number = 1;
        let recovered = flaky::retry(
            policy.and_then(|policy| policy.find("test-e2e", &case.key)),
            &case.name(),
            journal,
            &mut 0,
            || {
                attempt_number += 1;
                let report =
                    report.with_extension(format!("retry-{}-{attempt_number}.json", index + 1));
                let (success, value) = run_attempt(&retry_args(args, case)?, &report)?;
                let selected = cases(&value)?;
                check_exit(success, &value)?;
                let [selected] = selected.as_slice() else {
                    return Err("Playwright rerun did not select exactly one case".into());
                };
                if selected.selector() != case.selector() {
                    return Err("Playwright rerun selected a different case".into());
                }
                match (success, selected.status.as_str()) {
                    (true, "expected") if selected.passed => Ok(true),
                    (false, "unexpected") => Ok(false),
                    _ => Err("Playwright rerun did not complete the selected case".into()),
                }
            },
        )?;
        if !recovered {
            unresolved.push(case.name());
        }
    }
    if !unresolved.is_empty() {
        return Err(format!("Playwright failed: {}", unresolved.join(", ")).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
