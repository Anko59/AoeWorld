//! Fail closed unless the completed runner accounts for every failed test.
use super::{CARGO_DEADLINE, Result, SUITES};
use crate::process::{self, Cancellation, CaptureExit};
use std::{fs, path::Path, process::Command};

type SuiteOutcome = (
    std::result::Result<(), process::ProcessError>,
    Option<usize>,
);

pub(super) struct Failure {
    pub(super) error: Box<dyn std::error::Error>,
    pub(super) context_failures: Option<usize>,
}

impl From<Box<dyn std::error::Error>> for Failure {
    fn from(error: Box<dyn std::error::Error>) -> Self {
        Self {
            error,
            context_failures: None,
        }
    }
}

/// One pass only; the caller owns the single retry for the entire browser gate.
pub(super) fn run_suites(environment: &[(&str, &str)]) -> std::result::Result<(), Failure> {
    for suite in SUITES {
        let outcome = run_suite(&suite.cargo_arguments(), environment)?;
        if let (Err(error), context_failures) = outcome {
            return Err(Failure {
                error: error.into(),
                context_failures,
            });
        }
    }
    Ok(())
}

fn run_suite(arguments: &[String], environment: &[(&str, &str)]) -> Result<SuiteOutcome> {
    let directory = Path::new("reports/process");
    fs::create_dir_all(directory)?;
    // A failing runner repeats panic stacks in its console dump. Its bounded
    // diagnostic tail can lose failures; classify complete stdout, never that tail.
    let mut stdout = tempfile::Builder::new()
        .prefix("wasm-stdout-")
        .tempfile_in(directory)?;
    let mut command = Command::new("cargo");
    command.args(arguments).envs(environment.iter().copied());
    let captured = process::capture_command_retaining_stdout(
        command,
        CARGO_DEADLINE,
        &Cancellation::default(),
        stdout.as_file_mut(),
    );
    let mut context_failures = None;
    if !matches!(captured.exit, CaptureExit::Success) {
        let (_, path) = stdout.keep()?;
        eprintln!("test-wasm: complete runner stdout: {}", path.display());
        // Oversized or unreadable diagnostics are not evidence for a retry.
        if matches!(captured.exit, CaptureExit::Failed(Some(1)))
            && captured.stderr.len() < 64 * 1024
            && fs::metadata(&path).is_ok_and(|metadata| metadata.len() <= 8 * 1024 * 1024)
            && let Ok(mut output) = fs::read_to_string(&path)
        {
            output.push('\n');
            output.push_str(&String::from_utf8_lossy(&captured.stderr));
            context_failures = context_only_failures(&output);
        }
    }
    Ok((
        process::report_captured("cargo", captured, CARGO_DEADLINE),
        context_failures,
    ))
}

fn context_message(message: &str) -> bool {
    // Rust expect/unwrap prints its label followed by the context's quoted Err.
    // The error may arise during creation, upload, or drawing; the label is not
    // the evidence. Value assertions are rejected independently below.
    let Some(error) = message
        .rsplit_once(": \"")
        .and_then(|(_, error)| error.strip_suffix('"'))
    else {
        return false;
    };
    matches!(
        error,
        "WebGL2 graphics operation failed" | "WebGL2 unavailable" | "WebGL2 context lost"
    ) || error
        .strip_prefix("WebGL2 graphics error ")
        .is_some_and(|n| n.parse::<u32>().is_ok())
        || error.starts_with("WebGL2 shader: ")
        || error.starts_with("WebGL2 link: ")
}

/// Number of failures, only when all named failures and all panics are context errors.
pub(super) fn context_only_failures(output: &str) -> Option<usize> {
    let lines: Vec<_> = output.lines().map(str::trim).collect();
    if lines.iter().any(|line| {
        line.starts_with("assertion ")
            || line.starts_with("pixel ")
            || line.contains("RuntimeError:")
            || line.contains("fatal runtime error:")
            || line.starts_with("error[E")
            || (line.starts_with("error:")
                && !line.starts_with("error: test failed, to rerun pass "))
            || (line.starts_with("Error:") && *line != "Error: some tests failed")
            || line.contains("output capture truncated")
            || line.contains("earlier output truncated")
    }) {
        return None;
    }
    for (index, line) in lines.iter().enumerate() {
        if line.contains("panicked at ")
            && (!line.starts_with("panicked at ")
                || !lines
                    .get(index + 1)
                    .is_some_and(|message| context_message(message)))
        {
            return None;
        }
    }
    let summaries: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("test result:"))
        .collect();
    let [(summary_index, summary)] = summaries.as_slice() else {
        return None;
    };
    let failed = summary
        .strip_prefix("test result: FAILED. ")?
        .split("; ")
        .find_map(|part| part.strip_suffix(" failed"))?
        .parse::<usize>()
        .ok()?;
    if failed == 0 {
        return None;
    }
    let list_start = lines[..*summary_index]
        .iter()
        .rposition(|line| *line == "failures:")?;
    let names: Vec<_> = lines[list_start + 1..*summary_index]
        .iter()
        .filter(|line| !line.is_empty())
        .copied()
        .collect();
    let headers: Vec<_> = lines[..list_start]
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            line.strip_prefix("---- ")
                .and_then(|s| s.strip_suffix(" output ----"))
                .map(|name| (index, name))
        })
        .collect();
    if names.len() != failed || headers.len() != failed {
        return None;
    }
    for (position, (start, name)) in headers.iter().enumerate() {
        if names[position] != *name || names[..position].contains(name) {
            return None;
        }
        let end = headers
            .get(position + 1)
            .map_or(list_start, |(index, _)| *index);
        if lines[start + 1..end]
            .iter()
            .filter(|line| line.starts_with("panicked at "))
            .count()
            != 1
        {
            return None;
        }
    }
    Some(failed)
}
