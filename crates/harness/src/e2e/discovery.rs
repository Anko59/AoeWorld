//! Bound Playwright collection separately: no per-test timeout runs during import.
use crate::{
    process::{self, Cancellation, CaptureExit},
    ship::JsonOutput,
};
use std::{path::Path, process::Command, time::Duration};

#[cfg(test)]
mod tests;

pub(super) fn run(
    command: Command,
    report: &Path,
    deadline: Duration,
    cancellation: &Cancellation,
) -> Result<(), Box<dyn std::error::Error>> {
    // The listing is created exclusively; a report left by an earlier run of the same
    // checkout would otherwise stop every later filtered run with AlreadyExists.
    match std::fs::remove_file(report) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    let mut retained = JsonOutput::create(report, "Playwright discovery")?;
    let captured =
        process::capture_command_retaining_stdout(command, deadline, cancellation, &mut retained);
    // Collection that ends with an ordinary non-zero exit (a spec failed to load) is left
    // to the main run: its JSON report carries the load errors, which the compile-red
    // classifier judges. Only a hang, a crash or a cancellation fails here.
    if !matches!(
        captured.exit,
        CaptureExit::Success | CaptureExit::Failed(Some(1))
    ) {
        return Err(format!(
            "playwright-discovery unavailable: {:?} (deadline {}s)\n{}",
            captured.exit,
            deadline.as_secs(),
            super::stderr_tail(&captured.stderr)
        )
        .into());
    }
    Ok(())
}
