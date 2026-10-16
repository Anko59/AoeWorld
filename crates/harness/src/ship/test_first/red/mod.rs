//! Harness-owned proof from fresh runs, never from an agent's evidence files.
mod attribution;
mod record;
pub(crate) mod runner;

use super::{Kind, git, in_scope, is_test};
use crate::ship::duel::{baseline, focus::CheckReport};
pub(crate) use record::require;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Proof {
    pub(crate) tests: String,
    pub(crate) parent: String,
    pub(crate) paths: Vec<String>,
    pub(crate) failed: Vec<String>,
    pub(crate) parent_failed: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub(crate) head: String,
    pub(crate) base: String,
    pub(crate) proofs: Vec<Proof>,
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
    let _lock = crate::ship::lock::acquire(&root, "make red").map_err(|e| e.to_string())?;
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
    let report = super::report(root, base, head)?;
    let commits = candidates(&report)?;
    let cache = root.join(".cache/agent-hook/red");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut record = Record {
        head: head.into(),
        base: base.into(),
        proofs: Vec::new(),
    };
    for tests in commits {
        let parent = git::git(root, &["rev-parse", &format!("{tests}^")])?;
        let paths = changed_tests(root, &parent, tests)?;
        let mut diff = vec!["diff", "--name-only", tests, head, "--"];
        diff.extend(paths.iter().map(String::as_str));
        if !git::git(root, &diff)?.is_empty() {
            return Err(format!(
                "red: tests changed after {tests}; keep test bytes identical at HEAD"
            ));
        }
        // A tests-only commit is exactly its parent implementation plus its tests.
        let red_source = baseline::export_in(root, tests, &cache)?;
        let parent_source = baseline::export_in(root, &parent, &cache)?;
        println!(
            "red: tests {tests} on parent {parent}; {}",
            paths.join(", ")
        );
        let red = run(red_source.path(), &paths)?;
        let mut failed = validate_red(red_source.path(), parent_source.path(), &paths, &red)?;
        let parent_paths: Vec<_> = paths
            .iter()
            .filter(|path| parent_source.path().join(path).is_file())
            .cloned()
            .collect();
        let before = if parent_paths.is_empty() {
            CheckReport {
                passed: true,
                ..Default::default()
            }
        } else {
            println!("red: checking old tests on tests commit parent {parent}");
            run(parent_source.path(), &parent_paths)?
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
                "red: parent check unavailable; cannot exclude pre-existing failures".into(),
            );
        }
        failed.retain(|name| !before.failed.contains(name));
        if failed.is_empty() {
            return Err(
                "red: changed tests already failed on their own parent with old test files".into(),
            );
        }
        // Helper changes implicate their importing specs, whose test bytes must
        // also stay identical through HEAD.
        for name in &failed {
            if let Some((path, _)) = red.locations.get(name)
                && !git::git(root, &["diff", "--name-only", tests, head, "--", path])?.is_empty()
            {
                return Err(format!("red: tests changed after {tests}: {path}"));
            }
        }
        println!("red: FAIL (required): {}", failed.join(", "));
        let green_source = baseline::export_in(root, head, &cache)?;
        let green = run(green_source.path(), &paths)?;
        validate_green(&failed, &green)?;
        println!("green: PASS at {head}: {}", failed.join(", "));
        record.proofs.push(Proof {
            tests: tests.into(),
            parent,
            paths,
            failed,
            parent_failed: before.failed,
        });
    }
    record::write(root, &record)?;
    Ok(record)
}

fn candidates(report: &super::Report) -> git::Result<Vec<&str>> {
    let first_product = report.commits.iter().position(|(_, kind)| kind.product());
    let commits: Vec<_> = report
        .commits
        .iter()
        .enumerate()
        .filter(|(_, (_, kind))| *kind == Kind::TestsOnly)
        .map(|(i, (sha, _))| (i, sha.as_str()))
        .collect();
    if commits.is_empty() || first_product.is_some_and(|first| commits[0].0 >= first) {
        return Err(
            "red: no tests-only commit before implementation; commit failing tests first".into(),
        );
    }
    if report
        .commits
        .iter()
        .any(|(_, kind)| matches!(kind, Kind::Mixed | Kind::Merge(super::Base::Mixed)))
    {
        return Err("red: mixed product/test commit; commit failing tests first".into());
    }
    Ok(commits.into_iter().map(|(_, sha)| sha).collect())
}

fn changed_tests(root: &Path, parent: &str, tests: &str) -> git::Result<Vec<String>> {
    Ok(git::git_raw(
        root,
        &["diff", "--name-only", "-z", "--no-renames", parent, tests],
    )?
    .split('\0')
    .filter(|p| !p.is_empty() && in_scope(p) && is_test(p))
    .map(str::to_owned)
    .collect())
}

fn validate_red(
    root: &Path,
    old: &Path,
    paths: &[String],
    report: &CheckReport,
) -> git::Result<Vec<String>> {
    if report.unavailable.is_some()
        || report.build_failed
        || report
            .panics
            .iter()
            .any(|p| p.test.as_ref().is_none_or(|n| !report.failed.contains(n)))
    {
        return Err("red: unavailable/build/panic result is not a named failing test".into());
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
