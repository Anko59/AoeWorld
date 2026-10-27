use super::{
    Result, Target,
    routing::{self, Config},
    triage,
};
use crate::ship::{
    git,
    metrics::{Metrics, snapshot},
};
use serde::Deserialize;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const CONFIG: &str = "gates/metric-regressions.json";
const LISTED_FILES: usize = 20;

/// How far a metric may move before it is a regression.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Thresholds {
    version: u16,
    window_hours: u64,
    max_test_drop: u64,
    max_unsafe_increase: u64,
    max_todo_increase: u64,
    max_dependency_increase: u64,
    max_files_over_limit: u64,
}

impl Thresholds {
    pub(super) fn load(root: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(root.join(CONFIG))
            .map_err(|error| format!("{CONFIG}: {error}"))?;
        let thresholds: Self =
            serde_json::from_str(&text).map_err(|error| format!("{CONFIG}: {error}"))?;
        if thresholds.version != 1 || thresholds.window_hours == 0 {
            return Err(
                format!("{CONFIG}: version 1 and a positive window_hours are required").into(),
            );
        }
        Ok(thresholds)
    }
}

pub(in crate::ship) fn execute(root: &Path) -> Result<()> {
    if !crate::agents::main_session() {
        return Err("make metric-regressions is available only to the main session and CI".into());
    }
    let findings = collect(root)?;
    if findings.is_empty() {
        return Ok(());
    }
    let config = Config::load(root)?;
    let target = match Target::direct(root) {
        Ok(target) => target,
        Err(error) => {
            routing::warn(config.nightly, &error);
            return Ok(());
        }
    };
    let run = triage::run_link(&target.repo, std::env::var("GITHUB_RUN_ID").ok().as_deref());
    routing::comment(root, config.nightly, &body(&findings, &run));
    Ok(())
}

pub(super) fn collect(root: &Path) -> Result<Vec<String>> {
    let thresholds = Thresholds::load(root)?;
    let head = snapshot::compute(root, "HEAD")?.statics;
    let earlier = match baseline(root, thresholds.window_hours)? {
        Some(sha) => Some(snapshot::compute(root, &sha)?.statics),
        None => None,
    };
    Ok(regressions(earlier.as_ref(), &head, &thresholds))
}

/// The newest commit of HEAD's history made at least `hours` ago.
pub(super) fn baseline(root: &Path, hours: u64) -> Result<Option<String>> {
    if git::git(root, &["rev-parse", "--verify", "-q", "HEAD"]).is_err() {
        return Ok(None);
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let before = format!("--before={} +0000", now.saturating_sub(hours * 3600));
    let sha = git::git(root, &["rev-list", "-1", &before, "HEAD"])?;
    Ok((!sha.is_empty()).then_some(sha))
}

fn rise(label: &str, before: u64, after: u64, allowed: u64, findings: &mut Vec<String>) {
    if after > before + allowed {
        findings.push(format!(
            "{label} rose from {before} to {after} (+{}, allowed +{allowed})",
            after - before
        ));
    }
}

/// Every regression of `head` beyond the thresholds; without an earlier
/// snapshot only the absolute limit on file length applies.
pub(super) fn regressions(
    earlier: Option<&Metrics>,
    head: &Metrics,
    thresholds: &Thresholds,
) -> Vec<String> {
    let mut findings = Vec::new();
    if let Some(old) = earlier {
        let (before, after) = (old.test_total(), head.test_total());
        if before > after + thresholds.max_test_drop {
            findings.push(format!(
                "Test count fell from {before} to {after} (−{}, allowed −{})",
                before - after,
                thresholds.max_test_drop
            ));
        }
        rise(
            "`unsafe` lines",
            old.unsafe_count,
            head.unsafe_count,
            thresholds.max_unsafe_increase,
            &mut findings,
        );
        rise(
            "TODO/FIXME lines",
            old.todo_count,
            head.todo_count,
            thresholds.max_todo_increase,
            &mut findings,
        );
        rise(
            "Third-party dependencies",
            old.dependency_total(),
            head.dependency_total(),
            thresholds.max_dependency_increase,
            &mut findings,
        );
    }
    let over = &head.files_over_limit;
    if over.len() as u64 > thresholds.max_files_over_limit {
        let shown: Vec<_> = over
            .iter()
            .take(LISTED_FILES)
            .map(|p| format!("`{p}`"))
            .collect();
        findings.push(format!(
            "{} files exceed the 500-line limit: {}{}",
            over.len(),
            shown.join(", "),
            if over.len() > LISTED_FILES {
                ", …"
            } else {
                ""
            }
        ));
    }
    findings
}

fn body(findings: &[String], run: &str) -> String {
    let list: String = findings
        .iter()
        .map(|line| format!("- [ ] {line}\n"))
        .collect();
    format!(
        "Static metrics of dev regressed beyond `{CONFIG}` in {run}:\n\n{list}\nRecorded by `make metric-regressions` (docs/issues.md).\n"
    )
}

#[cfg(test)]
#[path = "tests/regressions.rs"]
mod tests;
