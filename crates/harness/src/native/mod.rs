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

#[cfg(test)]
mod tests;
