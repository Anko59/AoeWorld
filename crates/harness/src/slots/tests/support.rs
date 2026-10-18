//! Queue fixtures inject a no-op load wait; load tests inject readings and virtual time.
use crate::slots::{
    SlotEnv,
    guard::{self, Guard},
    queue::Result,
};
use std::path::Path;

pub(super) fn hold(from: &Path, gate: &str, env: &SlotEnv) -> Result<Guard> {
    match guard::checkout_of(from)? {
        Some(root) => guard::hold_in(&root, gate, env, || guard::slot_dir(&root, env), || Ok(())),
        None => guard::hold(from, gate, env),
    }
}

pub(super) fn hold_forced(from: &Path, gate: &str, env: &SlotEnv) -> Result<Guard> {
    let root = guard::checkout_of(from)?.expect("a checkout with a registry");
    guard::acquire_in(&root, gate, env, || guard::slot_dir(&root, env), || Ok(()))
}

pub(super) fn hold_snapshot(
    root: &Path,
    source: &Path,
    gate: &str,
    env: &SlotEnv,
) -> Result<Guard> {
    guard::hold_in(root, gate, env, || guard::slot_dir(source, env), || Ok(()))
}

pub(super) fn run_in_slot(from: &Path, gate: &str, argv: &[String], env: &SlotEnv) -> Result<i32> {
    guard::run_with_guard(argv, hold(from, gate, env)?)
}
