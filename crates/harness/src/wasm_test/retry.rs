//! Fail closed unless the completed runner accounts for every failed test.
use super::{CARGO_DEADLINE, Result, SUITES};
use crate::process::{self, Cancellation, CaptureExit};
use std::{fs, path::Path, process::Command};

type SuiteOutcome = (
    std::result::Result<(), process::ProcessError>,
    Option<Vec<String>>,
);

pub(super) struct Failure {
    pub(super) error: Box<dyn std::error::Error>,
    pub(super) context_failures: Option<Vec<String>>,
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
        if fs::metadata(&path).is_ok_and(|metadata| metadata.len() <= 8 * 1024 * 1024)
            && let Ok(output) = fs::read_to_string(&path)
        {
            context_failures = classify_completed(&captured.exit, &output, &captured.stderr);
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
    // the evidence. An assertion mentioning WebGL is not a context Err.
    if message.starts_with("assertion ") || message.starts_with("pixel ") {
        return false;
    }
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

/// Exit, capture bounds and UTF-8 are part of the retry decision, not parser hints.
pub(super) fn classify_completed(
    exit: &CaptureExit,
    stdout: &str,
    stderr: &[u8],
) -> Option<Vec<String>> {
    if !matches!(exit, CaptureExit::Failed(Some(1)))
        || stdout.len() > 8 * 1024 * 1024
        || stderr.len() >= 64 * 1024
    {
        return None;
    }
    let stderr = std::str::from_utf8(stderr).ok()?;
    context_failures(&format!("{stdout}\n{stderr}"))
}

/// Identify context panics in named failure blocks of a complete runner result.
/// Other failed tests do not veto the one retry; the second full pass decides.
pub(super) fn context_failures(output: &str) -> Option<Vec<String>> {
    let lines: Vec<_> = output.lines().map(str::trim).collect();
    if lines.iter().any(|line| {
        line.starts_with("error[E")
            || (line.starts_with("error:")
                && !line.starts_with("error: test failed, to rerun pass "))
            || line.contains("output capture truncated")
            || line.contains("earlier output truncated")
    }) {
        return None;
    }
    let summaries: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("test result:"))
        .collect();
    let [(summary_index, summary)] = summaries.as_slice() else {
        return None;
    };
    let parts: Vec<_> = summary
        .strip_prefix("test result: FAILED. ")?
        .split("; ")
        .collect();
    let [passed, failed, ignored, filtered, duration] = parts.as_slice() else {
        return None;
    };
    passed.strip_suffix(" passed")?.parse::<usize>().ok()?;
    let failed = failed.strip_suffix(" failed")?.parse::<usize>().ok()?;
    ignored.strip_suffix(" ignored")?.parse::<usize>().ok()?;
    filtered
        .strip_suffix(" filtered out")?
        .parse::<usize>()
        .ok()?;
    let seconds = duration
        .strip_prefix("finished in ")?
        .strip_suffix('s')?
        .parse::<f64>()
        .ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
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
    let mut contexts = Vec::new();
    let mut blocks = Vec::new();
    for (position, (start, name)) in headers.iter().enumerate() {
        if names[position] != *name || name.is_empty() || names[..position].contains(name) {
            return None;
        }
        let end = headers
            .get(position + 1)
            .map_or(list_start, |(index, _)| *index);
        let block = &lines[start + 1..end];
        // Both caught Rust panics and JS-only failures have runner failure details.
        if !block.iter().any(|line| {
            line.starts_with("panicked at ")
                || *line == "JS exception that was thrown:"
                || line.starts_with("note: ")
        }) {
            return None;
        }
        if block
            .windows(2)
            .any(|pair| pair[0].starts_with("panicked at ") && context_message(pair[1]))
        {
            contexts.push((*name).to_owned());
        }
        blocks.push((*start, end));
    }
    // The runner repeats test panic output in its console dump. Accept only
    // matching, named repeats; a runner panic/exception outside test details fails closed.
    let mut invoked = None;
    let mut console = false;
    for (index, line) in lines.iter().enumerate() {
        if *line == "console output:" && index > *summary_index {
            console = true;
        } else if *line == "--- stderr ---" || *line == "Error: some tests failed" {
            console = false;
            invoked = None;
        } else if console && let Some(name) = line.strip_prefix("Invoking test: ") {
            invoked = names.iter().position(|failed| *failed == name);
        }
        let in_block = blocks
            .iter()
            .any(|(start, end)| index > *start && index < *end);
        if line.contains("panicked at ") && !in_block {
            let position = invoked?;
            let (start, end) = blocks[position];
            let message = lines.get(index + 1)?;
            if !lines[start + 1..end]
                .windows(2)
                .any(|pair| pair == [*line, *message])
            {
                return None;
            }
        }
        if !in_block
            && (line.contains("RuntimeError:")
                || line.contains("fatal runtime error:")
                || (line.starts_with("Error:") && *line != "Error: some tests failed"))
        {
            return None;
        }
    }
    (!contexts.is_empty()).then_some(contexts)
}
