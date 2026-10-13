//! `make metric-regressions`: the nightly workflow compares the static metrics
//! of dev's HEAD with dev as of `window_hours` earlier and keeps ONE issue,
//! `Nightly: metric regressions`, listing every change beyond the thresholds in
//! `gates/metric-regressions.json`. It opens or updates that issue while there
//! is a regression and closes it when there is none. Ownership follows
//! [`triage::owned_titled`]: only the bot's labelled, marked issue is touched.
use super::{Issue, Result, Target, triage};
use crate::ship::{
    git,
    metrics::{Metrics, snapshot},
};
use serde::Deserialize;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const TITLE: &str = "Nightly: metric regressions";
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
    let thresholds = Thresholds::load(root)?;
    let head = snapshot::compute(root, "HEAD")?.statics;
    let earlier = match baseline(root, thresholds.window_hours)? {
        Some(sha) => Some(snapshot::compute(root, &sha)?.statics),
        None => None,
    };
    let findings = regressions(earlier.as_ref(), &head, &thresholds);
    let target = Target::load(root)?;
    let run = triage::run_link(&target.repo, std::env::var("GITHUB_RUN_ID").ok().as_deref());
    let mut problems = Vec::new();
    for action in plan(&findings, &target.issues, &run) {
        match triage::apply(&target, root, &action) {
            Ok(_) => println!("metric-regressions: {action:?}"),
            Err(error) => problems.push(format!("{action:?}: {error}")),
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    Err(format!("metric-regressions: {}", problems.join("; ")).into())
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
    let list: String = findings.iter().map(|line| format!("- {line}\n")).collect();
    format!(
        "Static metrics of dev regressed beyond `{CONFIG}` in {run}:\n\n{list}\n`make metric-regressions` updates this issue on every nightly run and closes it when none remains (docs/issues.md).\n"
    )
}

pub(super) fn plan(findings: &[String], issues: &[Issue], run: &str) -> Vec<triage::Action> {
    let mut mine = issues
        .iter()
        .filter(|issue| triage::owned_titled(issue, TITLE));
    if findings.is_empty() {
        return mine
            .map(|issue| triage::Action::Close {
                number: issue.number,
                comment: format!("No metric regression in {run}."),
            })
            .collect();
    }
    vec![match mine.next() {
        Some(issue) => triage::Action::Update {
            number: issue.number,
            title: TITLE.to_owned(),
            body: body(findings, run),
        },
        None => triage::Action::Open {
            title: TITLE.to_owned(),
            body: body(findings, run),
        },
    }]
}

#[cfg(test)]
#[path = "tests/regressions.rs"]
mod tests;
