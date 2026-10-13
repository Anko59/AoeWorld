//! Runtime reports and red/green commands use bounded process-group capture.
use super::{Options, focus::CheckReport};
use crate::{
    agents::{Runtime, launch},
    process::{Cancellation, CaptureExit, Captured, capture_command, excerpt},
    ship::git,
};
use std::{fs, path::Path, process::Command, time::Duration};

/// Longest output tail a summary or prompt embeds.
const TAIL_LINES: usize = 20;
/// Most failing test names a status line lists.
const NAMES: usize = 20;

/// The failure lines of a command's output, else its last lines: never a passing test, at most 20 lines.
pub(crate) fn tail(c: &Captured) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&c.stdout),
        String::from_utf8_lossy(&c.stderr)
    );
    excerpt(&text)
}

pub(crate) fn excerpt(text: &str) -> String {
    let mut lines = excerpt::failure_blocks(text);
    if lines.is_empty() {
        lines = text
            .lines()
            .filter(|line| !line.trim().is_empty() && !excerpt::is_passing_line(line))
            .map(str::to_owned)
            .collect();
    }
    let lines: Vec<String> = lines[lines.len().saturating_sub(TAIL_LINES)..]
        .iter()
        .map(|line| match line.char_indices().nth(240) {
            Some((end, _)) => format!("{}…", &line[..end]),
            None => line.clone(),
        })
        .collect();
    bounded(&lines.join("\n"))
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

/// Run a trusted main-session command (DUEL_CHECK, DUEL_QUALIFY), never a runtime's shell text.
pub(crate) fn shell(root: &Path, command: &str, budget: Duration) -> Captured {
    eprintln!("duel: {command}");
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
    capture_command(c, budget, &Cancellation::default())
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
    let captured = shell(root, command, options.budget);
    let tail = tail(&captured);
    let passed = matches!(captured.exit, CaptureExit::Success);
    let stored = fs::read(&report_path)
        .ok()
        .and_then(|v| serde_json::from_slice::<CheckReport>(&v).ok());
    let mut report = if let Some(mut report) = stored {
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
    if !focused {
        // Explicit main-session checks decide red/green solely by normal exit status.
        report.passed = passed;
        report.unavailable = None;
    }
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

/// What a status line says about the tests: the failing names, else a count, never a list of passing tests.
pub(crate) fn names(report: &CheckReport) -> String {
    if !report.failed.is_empty() {
        let mut shown = report
            .failed
            .iter()
            .take(NAMES)
            .cloned()
            .collect::<Vec<_>>();
        if report.failed.len() > NAMES {
            shown.push(format!("(+{} more)", report.failed.len() - NAMES));
        }
        shown.join(", ")
    } else if let Some(panic) = report.panics.first() {
        let more = report.panics.len() - 1;
        let more = if more > 0 {
            format!(" (+{more} more)")
        } else {
            String::new()
        };
        format!(
            "panicked at {}: {}{more}",
            panic.location,
            panic.message.lines().next().unwrap_or("")
        )
    } else if !report.tests.is_empty() {
        format!("{} passed", report.tests.len())
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
        report.failed = excerpt::failed_test_names(output);
    }
    if report.tests.is_empty() {
        report.tests = excerpt::passed_test_names(output);
        report.tests.extend(report.failed.iter().cloned());
    }
    report
}
