//! Runs of the game score in child processes: the engine's thread pool is
//! fixed once per process, so each worker count needs a process of its own.
use super::judge::{self, Comparison};
use super::measure::Measurement;
use super::{BoxError, Plan};
use std::{fs, path::Path, process::Command};

/// Plays `plan` in a child process and reads back what it measured.
pub(super) fn run(plan: &Plan, out: &Path) -> Result<Measurement, BoxError> {
    let status = Command::new(std::env::current_exe()?)
        .args(plan.child_args(out))
        .status()?;
    if !status.success() {
        return Err(format!(
            "game-score: the child at {} workers failed: {status}",
            plan.workers
        )
        .into());
    }
    Ok(serde_json::from_str(&fs::read_to_string(out)?)?)
}

/// Plays `plan` again at its worker count and at `other_workers`, to find
/// the exact first tick where the two differ (`plan` hashes every tick). The
/// files are named from `prefix`.
pub(super) fn diagnose(
    plan: &Plan,
    other_workers: usize,
    directory: &Path,
    prefix: &str,
) -> Result<Comparison, BoxError> {
    let base = run(
        plan,
        &directory.join(format!("{prefix}diagnosis-base.json")),
    )?;
    let other = Plan {
        workers: other_workers,
        ..plan.clone()
    };
    let other = run(
        &other,
        &directory.join(format!("{prefix}diagnosis-other.json")),
    )?;
    let (base_orders, other_orders) = (judge::read_orders(&base), judge::read_orders(&other));
    Ok(judge::compare(
        &base,
        &other,
        (base_orders.as_deref(), other_orders.as_deref()),
    ))
}
