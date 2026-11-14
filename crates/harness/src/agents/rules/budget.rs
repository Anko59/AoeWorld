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
    // It runs test-unit, test-wasm and test-e2e one after another.
    "test-client-memory",
];
/// Focused targets: allowed with a bounded filter, leased like a suite otherwise.
pub(crate) const FOCUSED: &[&str] = &[
    "test-native-focused",
    "test-harness-focused",
    "test-wasm-focused",
    "test-e2e",
];
/// The most specs a focused browser run may name.
pub(crate) const FOCUSED_SPECS: usize = 5;
/// The longest lease a coordinator may grant.
pub(crate) const MAX_LEASE_MINUTES: u64 = 240;

pub(crate) fn check(context: &Context, target: &str, assignments: &[(&str, &str)]) -> Verdict {
    if !context.budgeted() {
        return Ok(());
    }
    if target == "lease" {
        return Err(
            "a lease is the coordinator's decision: ask the session that launched you".into(),
        );
    }
    if let Some(reason) = unbounded(target, assignments) {
        if leased(&context.common, target, now()) {
            return Ok(());
        }
        return Err(format!(
            "`make {target}` with {reason} runs a whole suite under a focused name: name a \
             package and a module or test filter, or ask your coordinator for a lease \
             (`make lease AOE_LEASE_TARGET={target}`)"
        ));
    }
    if !SUITES.contains(&target) || leased(&context.common, target, now()) {
        return Ok(());
    }
    Err(format!(
        "`make {target}` is a whole suite or sweep, and CI runs it on the pull request: run \
         the focused form (test-native-focused, test-harness-focused, test-wasm-focused, or \
         test-e2e with 1 to {FOCUSED_SPECS} AOE_E2E_SPECS files), then commit and report \
         back. Your coordinator can lend it with `make lease AOE_LEASE_TARGET={target}`"
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
    if !SUITES.contains(&target) && !FOCUSED.contains(&target) {
        return Err(format!(
            "`{target}` needs no lease; leases cover {} and {}",
            SUITES.join(", "),
            FOCUSED.join(", ")
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

/// The value Make uses: the last assignment of `name` wins.
fn last<'a>(assignments: &[(&str, &'a str)], name: &str) -> Option<&'a str> {
    assignments
        .iter()
        .rev()
        .find(|(n, _)| *n == name)
        .map(|(_, value)| *value)
}

/// Why a focused target's selection is not bounded, if it is not: no filter, a
/// nextest expression without a `test(...)` predicate of at least three
/// characters, `all()`, a short substring, or e2e specs that are not 1 to 5
/// concrete spec files (2026-11-13: an agent ran `test-native-focused` with
/// `all()` and held a slot through the out-of-memory crash).
fn unbounded(target: &str, assignments: &[(&str, &str)]) -> Option<String> {
    match target {
        "test-native-focused" => {
            let filter = last(assignments, "AOE_NATIVE_TEST_FILTER").unwrap_or_default();
            (!positive_union(filter)).then(|| {
                format!(
                    "AOE_NATIVE_TEST_FILTER={filter:?}: a focused filter is test(...) predicates of at \
                     least {SHORTEST_FILTER} characters joined by `|`; negation, difference, \
                     intersection and other predicates can select a whole package"
                )
            })
        }
        "test-harness-focused" | "test-wasm-focused" => {
            let variable = if target == "test-harness-focused" {
                "HARNESS_TEST_FILTER"
            } else {
                "AOE_WASM_TEST_FILTER"
            };
            let filter: String = last(assignments, variable)
                .unwrap_or_default()
                .split_whitespace()
                .collect();
            (filter.len() < SHORTEST_FILTER || filter.contains("all()"))
                .then(|| format!("{variable}={filter:?}, which matches nearly every test"))
        }
        "test-e2e" => {
            let specs: Vec<&str> = last(assignments, "AOE_E2E_SPECS")
                .unwrap_or_default()
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|spec| !spec.is_empty())
                .collect();
            let concrete = specs.iter().all(|spec| {
                spec.ends_with(".spec.ts") && !spec.contains(['*', '?', '[', ']', '{', '}'])
            });
            (!(1..=FOCUSED_SPECS).contains(&specs.len()) || !concrete).then(|| {
                format!("AOE_E2E_SPECS that is not 1 to {FOCUSED_SPECS} concrete .spec.ts files")
            })
        }
        _ => None,
    }
}

/// A nextest expression that only adds named tests: one or more `test(...)`
/// predicates of at least three characters, joined by `|`, `+` or `or`. An
/// allowlist, so `not`, `-`, `&`, `all()`, `kind()` and anything new need a lease.
fn positive_union(filter: &str) -> bool {
    let mut rest = filter;
    let mut joiners = String::new();
    let mut predicates = 0;
    while let Some(start) = rest.find("test(") {
        joiners.push_str(&rest[..start]);
        let inner_start = start + "test(".len();
        let Some(length) = rest[inner_start..].find(')') else {
            return false;
        };
        let inner = rest[inner_start..inner_start + length]
            .trim()
            .trim_start_matches(['=', '~', '/'])
            .trim_end_matches('/');
        if inner.len() < SHORTEST_FILTER {
            return false;
        }
        predicates += 1;
        rest = &rest[inner_start + length + 1..];
    }
    joiners.push_str(rest);
    predicates > 0
        && joiners
            .split(|c: char| c.is_whitespace() || c == '|' || c == '+')
            .all(|word| word.is_empty() || word == "or")
}
