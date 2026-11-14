//! Whole suites and sweeps for agents and launched sessions: CI runs them on every
//! pull request, so an agent runs the focused form, or a coordinator lends one with
//! `make lease`. The lease is a file in the Git common directory, where agents
//! cannot write.
use crate::agents::context::{Context, Verdict};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Whole suites and sweeps: CI runs each on a pull request, a merge or a night.
pub(crate) const SUITES: &[&str] = &[
    "preflight",
    "preflight-checks",
    "preflight-unit",
    "test-unit",
    "test-harness",
    "test-wasm",
    "coverage",
    "fuzz-smoke",
    "fuzz-nightly",
    "test-exhaustive",
    "test-source-qualification",
    "test-geographic-matrix",
    "perf-ci",
    "perf-full",
    "perf-stress",
    "perf-scale",
    "perf-soak-10",
    "perf-soak-30",
    "perf-baseline-propose",
    "release-build",
];
/// The most specs a focused browser run may name.
pub(crate) const FOCUSED_SPECS: usize = 5;
/// The longest lease a coordinator may grant.
pub(crate) const MAX_LEASE_MINUTES: u64 = 240;

pub(crate) fn check(context: &Context, target: &str, assignments: &[(&str, &str)]) -> Verdict {
    if !context.budgeted() {
        return Ok(());
    }
    if let Some(reason) = unbounded(target, assignments) {
        if leased(&context.common, target, now()) {
            return Ok(());
        }
        return Err(format!(
            "`make {target}` with {reason} runs a whole suite under a focused name: name a \
             package and a module or test filter, or take a lease with `make lease \
             AOE_LEASE_TARGET={target}`"
        ));
    }
    if target == "test-e2e" {
        let specs = assignments
            .iter()
            .filter(|(name, _)| *name == "AOE_E2E_SPECS")
            .map(|(_, value)| value.split([',', ' ']).filter(|s| !s.is_empty()).count())
            .sum::<usize>();
        if (1..=FOCUSED_SPECS).contains(&specs) {
            return Ok(());
        }
    } else if !SUITES.contains(&target) {
        return Ok(());
    }
    if leased(&context.common, target, now()) {
        return Ok(());
    }
    Err(format!(
        "`make {target}` is a whole suite or sweep, and CI runs it on the pull request: run \
         the focused form (test-native-focused, test-harness-focused, test-wasm-focused, or \
         test-e2e with 1 to {FOCUSED_SPECS} AOE_E2E_SPECS), then commit and report back. A \
         coordinator can lend it for a while with `make lease AOE_LEASE_TARGET={target}`"
    ))
}

pub(crate) fn lease_path(common: &Path, target: &str) -> PathBuf {
    common.join("aoe-ship/leases").join(target)
}

/// A lease file holds its expiry in Unix seconds, then who granted it.
pub(crate) fn leased(common: &Path, target: &str, now: u64) -> bool {
    fs::read_to_string(lease_path(common, target))
        .ok()
        .and_then(|text| text.lines().next()?.trim().parse::<u64>().ok())
        .is_some_and(|until| until > now)
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// `make lease`: let launched sessions run one whole suite for `minutes`.
pub(crate) fn grant(
    common: &Path,
    target: &str,
    minutes: u64,
    by: &str,
) -> Result<PathBuf, String> {
    if target != "test-e2e" && !SUITES.contains(&target) {
        return Err(format!(
            "`{target}` needs no lease; leases cover {}",
            SUITES.join(", ")
        ));
    }
    if !(1..=MAX_LEASE_MINUTES).contains(&minutes) {
        return Err(format!(
            "a lease lasts 1 to {MAX_LEASE_MINUTES} minutes, not {minutes}"
        ));
    }
    let path = lease_path(common, target);
    fs::create_dir_all(path.parent().ok_or("lease has no directory")?)
        .map_err(|e| e.to_string())?;
    fs::write(&path, format!("{}\n{by}\n", now() + minutes * 60)).map_err(|e| e.to_string())?;
    Ok(path)
}

/// The shortest substring filter that still names a module or test.
const SHORTEST_FILTER: usize = 3;

/// A focused target whose filter selects everything: `all()` anywhere in a nextest
/// expression, or a substring too short to name anything (2026-11-13: an agent ran
/// `test-native-focused` with `all()` and held a slot through the out-of-memory
/// crash).
fn unbounded(target: &str, assignments: &[(&str, &str)]) -> Option<String> {
    let variable = match target {
        "test-native-focused" => "AOE_NATIVE_TEST_FILTER",
        "test-harness-focused" => "HARNESS_TEST_FILTER",
        "test-wasm-focused" => "AOE_WASM_TEST_FILTER",
        _ => return None,
    };
    let filter = assignments
        .iter()
        .rev()
        .find(|(name, _)| *name == variable)
        .map(|(_, value)| value.split_whitespace().collect::<String>());
    match filter.as_deref() {
        None | Some("") => Some(format!("no {variable}")),
        Some(filter) if filter.contains("all()") => Some(format!("{variable}=all()")),
        Some(filter) if filter.len() < SHORTEST_FILTER => Some(format!(
            "{variable}={filter:?}, which matches nearly every test"
        )),
        Some(_) => None,
    }
}
