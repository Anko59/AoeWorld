//! Runtime reports and red/green commands use bounded process-group capture.
use super::{Options, focus::CheckReport};
use crate::{
    agents::{Runtime, launch},
    process::{Cancellation, CaptureExit, Captured, capture_command},
    ship::git,
};
use std::{fs, path::Path, process::Command};

pub(crate) fn tail(c: &Captured) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    );
    bounded(
        &text
            .lines()
            .rev()
            .take(40)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub(crate) fn bounded(text: &str) -> String {
    let mut start = text.len().saturating_sub(8000);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}

pub(crate) fn report(runtime: Runtime, output: &str) -> String {
    let values: Vec<serde_json::Value> = output
        .lines()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect();
    for value in values.iter().rev() {
        let text = match runtime {
            Runtime::Claude => value
                .get("result")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            Runtime::Codex => value
                .get("item")
                .filter(|v| v["type"] == "agent_message")
                .and_then(|v| v["text"].as_str())
                .map(str::to_owned),
            Runtime::Pi => value
                .get("message")
                .filter(|v| v["role"] == "assistant")
                .and_then(|v| v["content"].as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|v| v["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                }),
            Runtime::Dsh => None,
        };
        if let Some(text) = text {
            return bounded(&text);
        }
    }
    bounded(output)
}

pub(crate) fn agent(
    root: &Path,
    options: &Options,
    role: &str,
    prompt: &str,
) -> git::Result<(CaptureExit, String)> {
    eprintln!(
        "duel: launching {} {role} (budget {}s)",
        options.runtime.label(),
        options.budget.as_secs()
    );
    let command = launch::work(
        options.runtime,
        root,
        role,
        prompt,
        options.model.as_deref(),
        options.effort.as_deref(),
    )?;
    let captured = capture_command(command, options.budget, &Cancellation::default());
    let output = String::from_utf8_lossy(&captured.stdout);
    Ok((captured.exit, report(options.runtime, &output)))
}

pub(crate) fn check(
    root: &Path,
    options: &Options,
    command: &str,
    focused: bool,
) -> git::Result<(CheckReport, String)> {
    let report_path = root.join(".cache/duel/check.json");
    if report_path.exists() {
        fs::remove_file(&report_path).map_err(|e| e.to_string())?;
    }
    eprintln!("duel: {command}");
    // DUEL_CHECK is the person's trusted command, never a runtime's shell text.
    let mut c = Command::new("sh");
    c.args(["-c", command]).current_dir(root);
    for key in [
        "MAKEFLAGS",
        "MFLAGS",
        "MAKEOVERRIDES",
        "MAKEFILES",
        "GNUMAKEFLAGS",
    ] {
        c.env_remove(key);
    }
    let captured = capture_command(c, options.budget, &Cancellation::default());
    let tail = tail(&captured);
    let passed = matches!(captured.exit, CaptureExit::Success);
    let stored = fs::read(&report_path)
        .ok()
        .and_then(|v| serde_json::from_slice::<CheckReport>(&v).ok());
    let report = if let Some(mut report) = stored {
        report.passed &= passed;
        report
    } else if focused {
        return Err(format!("focused check produced no readable result\n{tail}"));
    } else {
        let output = format!(
            "{}\n{}",
            String::from_utf8_lossy(&captured.stdout),
            String::from_utf8_lossy(&captured.stderr)
        );
        override_report(&output, passed)
    };
    if let Some(reason) = &report.unavailable {
        return Err(format!("check could not complete: {reason}\n{tail}"));
    }
    match captured.exit {
        CaptureExit::Success | CaptureExit::Failed(Some(_)) => Ok((report, tail)),
        other => Err(format!("check could not complete: {other:?}\n{tail}")),
    }
}

pub(crate) fn default_check(base: &str) -> String {
    format!("make duel-check HARNESS_DUEL_BASE={base}")
}

pub(crate) fn names(report: &CheckReport) -> String {
    if !report.failed.is_empty() {
        report.failed.join(", ")
    } else if !report.tests.is_empty() {
        report.tests.join(", ")
    } else {
        "unidentified (see check output)".into()
    }
}

pub(super) fn override_report(output: &str, passed: bool) -> CheckReport {
    let mut report = super::focus::rust_results(output);
    report.passed = passed;
    report.build_failed = !passed
        && report.tests.is_empty()
        && output
            .lines()
            .any(|line| line.trim_start().starts_with("error: could not compile "));
    if report.failed.is_empty() {
        for line in output.lines().map(str::trim) {
            if let Some(status) = line
                .strip_prefix("FAIL ")
                .or_else(|| line.strip_prefix("TIMEOUT "))
            {
                let name = status
                    .split_once(']')
                    .map_or(status, |(_, name)| name)
                    .trim();
                let name = name
                    .strip_prefix('(')
                    .and_then(|s| s.split_once(')'))
                    .map_or(name, |(_, name)| name)
                    .trim();
                if !name.is_empty() {
                    report.failed.push(name.to_owned());
                }
            }
        }
        report.failed.sort();
        report.failed.dedup();
    }
    report
}
