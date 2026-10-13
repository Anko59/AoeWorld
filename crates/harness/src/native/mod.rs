//! Fixed native test policy with final failure output, not authenticated evidence.
use crate::process::{self, ProcessError};
use std::{path::Path, time::Duration};

const ARGS: &[&str] = &[
    "nextest",
    "run",
    "--workspace",
    "--locked",
    "--no-fail-fast",
    "--failure-output",
    "final",
    "--success-output",
    "never",
];
const BUDGET: Duration = Duration::from_secs(600);

/// Preserve the existing test set and deadline. Explicit reporter arguments
/// override inherited Nextest reporter settings, without increasing capture size.
/// Git environment clearing belongs to process::run_in; other credentials and
/// PATH are not isolated by this helper.
pub(crate) fn run_in(root: &Path, environment: &[(&str, &str)]) -> Result<(), ProcessError> {
    process::run_in(root, "cargo", ARGS, environment, BUDGET)
}

pub(crate) fn lint() -> Result<(), ProcessError> {
    let args = ["clippy", "--workspace", "--all-targets", "--locked"];
    process::run(
        "cargo",
        &[args.as_slice(), &["--", "-D", "warnings"]].concat(),
        BUDGET,
    )
}

pub(crate) fn test_unit() -> Result<(), ProcessError> {
    run_in(Path::new("."), &[])?;
    process::run(
        "cargo",
        &["test", "--workspace", "--doc", "--locked"],
        BUDGET,
    )
}

#[cfg(test)]
mod tests;
