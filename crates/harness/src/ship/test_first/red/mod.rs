//! Harness-owned proof from fresh runs, never from an agent's evidence files.
mod attribution;
mod compile;
mod mounts;
mod record;
pub(crate) mod runner;
mod snapshot;

use super::{git, in_scope, is_test};
use crate::ship::duel::{baseline, focus::CheckReport};
pub(crate) use record::require;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// One PR-level proof: the PR's whole set of changed tests, red on the merge
/// base's product code and green at `head`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub(crate) head: String,
    pub(crate) base: String,
    /// The test files the PR changes (merge base → head), and their blob ids at head.
    pub(crate) paths: Vec<String>,
    pub(crate) blobs: Vec<String>,
    /// Changed tests that failed by name on the merge base's product code.
    pub(crate) failed: Vec<String>,
    /// API items the PR adds that a changed test calls, when the red was a compile failure.
    pub(crate) missing: Vec<String>,
    /// Changed tests of the files that failed to compile at red, seen running and passing at head.
    pub(crate) ran: Vec<String>,
    /// Failures already present at the merge base with the old test files (never evidence).
    pub(crate) base_failed: Vec<String>,
}

/// Optional explicit roots/revisions support rehearsals without moving a branch.
#[derive(Debug, clap::Args)]
pub(crate) struct Options {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long, default_value = "origin/dev")]
    base: String,
    #[arg(long, default_value = "HEAD")]
    head: String,
}

fn resolve(root: &Path, revision: &str) -> git::Result<String> {
    git::git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
    )
}

pub(crate) fn execute(options: &Options, slots: &crate::slots::SlotEnv) -> git::Result<()> {
    if !crate::agents::main_session() {
        return Err("make red belongs to the main session only".into());
    }
    let env = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    let root = env("HARNESS_RED_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| options.root.clone());
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let head = resolve(
        &root,
        &env("HARNESS_RED_HEAD").unwrap_or_else(|| options.head.clone()),
    )?;
    let base = env("HARNESS_RED_BASE").unwrap_or_else(|| options.base.clone());
    let base = git::git(&root, &["merge-base", &resolve(&root, &base)?, &head])?;
    // No ship lock: the red runs build and test under a heavy slot, which serializes the heavy
    // work, and the record is keyed by the exact head. Ships waiting to push never wait on it.
    let _slot = crate::slots::hold(&root, "test-e2e", slots).map_err(|e| e.to_string())?;
    let record = run_with(&root, &base, &head, &mut |snapshot, paths| {
        runner::run(&root, snapshot, paths)
    })?;
    println!(
        "RED_RECORD {}",
        serde_json::to_string(&record).map_err(|e| e.to_string())?
    );
    Ok(())
}

pub(crate) fn run_with(
    root: &Path,
    base: &str,
    head: &str,
    run: &mut impl FnMut(&Path, &[String]) -> git::Result<CheckReport>,
) -> git::Result<Record> {
    let paths = changed_tests(root, base, head)?;
    if paths.is_empty() {
        return Err(
            "red: the PR changes no test file; add a test that fails without the product change, or declare Harness-Test-First: exempt".into(),
        );
    }
    let cache = root.join(".cache/agent-hook/red");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    // The red tree is the merge base's product code plus ONLY the PR's test files.
    let red_source = baseline::export_in(root, base, &cache)?;
    snapshot::overlay(root, head, red_source.path(), &paths)?;
    mounts::apply(root, base, head, red_source.path(), &paths)?;
    let base_source = baseline::export_in(root, base, &cache)?;
    println!(
        "red: the PR's tests on merge base {base}; {}",
        paths.join(", ")
    );
    let red = run(red_source.path(), &paths)?;
    let (mut failed, missing) = if red.build_failed {
        (
            Vec::new(),
            compile::missing(root, base, head, &paths, &red)?,
        )
    } else {
        (
            validate_red(red_source.path(), base_source.path(), &paths, &red)?,
            Vec::new(),
        )
    };
    let existing: Vec<_> = paths
        .iter()
        .filter(|path| base_source.path().join(path).is_file())
        .cloned()
        .collect();
    let before = if existing.is_empty() {
        CheckReport {
            passed: true,
            ..Default::default()
        }
    } else {
        println!("red: checking the old tests on merge base {base}");
        run(base_source.path(), &existing)?
    };
    if before.unavailable.is_some()
        || before.build_failed
        || before
            .panics
            .iter()
            .any(|p| p.test.as_ref().is_none_or(|n| !before.failed.contains(n)))
        || (!before.passed && before.failed.is_empty())
    {
        return Err(
            "red: merge base check unavailable; cannot exclude pre-existing failures".into(),
        );
    }
    failed.retain(|name| !before.failed.contains(name));
    if failed.is_empty() && missing.is_empty() {
        return Err(
            "red: the changed tests already failed at the merge base with the old test files"
                .into(),
        );
    }
    if missing.is_empty() {
        println!("red: FAIL (required): {}", failed.join(", "));
    } else {
        println!(
            "red: COMPILE FAIL (required): tests need {}",
            missing.join(", ")
        );
    }
    let green_source = baseline::export_in(root, head, &cache)?;
    let green = run(green_source.path(), &paths)?;
    validate_green(&failed, &green)?;
    let ran = if missing.is_empty() {
        Vec::new()
    } else {
        compile::ran(green_source.path(), base_source.path(), &red, &green)?
    };
    println!(
        "green: PASS at {head}: {}",
        failed
            .iter()
            .chain(&ran)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );
    let record = Record {
        head: head.into(),
        base: base.into(),
        blobs: blobs(root, head, &paths)?,
        paths,
        failed,
        missing,
        ran,
        base_failed: before.failed,
    };
    record::write(root, &record)?;
    Ok(record)
}

/// The in-scope test files the PR's whole diff adds, changes or deletes.
fn changed_tests(root: &Path, base: &str, head: &str) -> git::Result<Vec<String>> {
    Ok(git::git_raw(
        root,
        &["diff", "--name-only", "-z", "--no-renames", base, head],
    )?
    .split('\0')
    .filter(|p| !p.is_empty() && in_scope(p) && is_test(p))
    .map(str::to_owned)
    .collect())
}

/// The blob id of each path at `head` (empty for a deleted file).
fn blobs(root: &Path, head: &str, paths: &[String]) -> git::Result<Vec<String>> {
    paths
        .iter()
        .map(|path| {
            Ok(
                git::git(root, &["rev-parse", "--verify", &format!("{head}:{path}")])
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn validate_red(
    root: &Path,
    old: &Path,
    paths: &[String],
    report: &CheckReport,
) -> git::Result<Vec<String>> {
    if report.unavailable.is_some()
        || report
            .panics
            .iter()
            .any(|p| p.test.as_ref().is_none_or(|n| !report.failed.contains(n)))
    {
        return Err("red: unavailable/panic result is not a named failing test".into());
    }
    let mut failed = Vec::new();
    for name in &report.failed {
        if attribution::changed(root, old, paths, report, name)? {
            failed.push(name.clone());
        }
    }
    if report.passed || failed.is_empty() {
        return Err("red: changed tests already pass or no changed spec/test failed".into());
    }
    Ok(failed)
}

fn validate_green(failed: &[String], green: &CheckReport) -> git::Result<()> {
    if !green.passed
        || green.build_failed
        || green.unavailable.is_some()
        || !green.failed.is_empty()
        || !green.panics.is_empty()
        || failed.iter().any(|name| !green.tests.contains(name))
    {
        return Err(
            "red: green check failed or a failing test did not run and pass at HEAD".into(),
        );
    }
    Ok(())
}
