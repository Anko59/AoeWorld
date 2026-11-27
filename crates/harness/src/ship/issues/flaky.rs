//! Flaky e2e tests found by the nightly triage: a test that CI recorded as flaky (it failed,
//! then passed on its one immediate retry) in three runs within seven days gets an issue.
//!
//! The record is `reports/e2e/*.flaky.jsonl` of each CI e2e shard, uploaded as the artifact
//! `e2e-flaky-<shard>` (kept 14 days) only when a shard saw a flake.
use super::{Result, Target, triage::run_link};
use crate::gates::flaky::date;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

/// Runs that must flag a test within the window for it to be reported.
const THRESHOLD: usize = 3;
const WINDOW_DAYS: i64 = 7;
const ARTIFACT_PREFIX: &str = "e2e-flaky-";
const PAGE_CAP: u32 = 30;
const ZIP_LIMIT: u64 = 1 << 20;

#[derive(Debug, Deserialize)]
struct Page {
    artifacts: Vec<Artifact>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Artifact {
    id: u64,
    name: String,
    #[serde(default)]
    expired: bool,
    created_at: String,
    workflow_run: Option<Run>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct Run {
    id: u64,
}

/// One line of a `.flaky.jsonl` file; the other fields are for people.
#[derive(Debug, Deserialize)]
struct Line {
    test: String,
    project: String,
    #[serde(default)]
    error: String,
}

/// A test flagged flaky by one artifact (one shard of one run attempt).
#[derive(Debug, PartialEq)]
struct Flag {
    test: String,
    project: String,
    error: String,
    artifact: u64,
    run: Option<u64>,
}

/// A test flagged in at least [`THRESHOLD`] artifacts.
#[derive(Debug, PartialEq)]
struct Repeat {
    test: String,
    project: String,
    runs: Vec<Option<u64>>,
    errors: Vec<String>,
}

fn today() -> Result<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() / 86_400,
    )?)
}

/// The flaky-record artifacts created within the window, newest first as listed.
fn recent(artifacts: &[Artifact], today: i64) -> Vec<&Artifact> {
    artifacts
        .iter()
        .filter(|artifact| {
            artifact.name.starts_with(ARTIFACT_PREFIX)
                && !artifact.expired
                && artifact
                    .created_at
                    .get(..10)
                    .and_then(|day| date(day).ok())
                    .is_some_and(|day| (0..=WINDOW_DAYS).contains(&(today - day)))
        })
        .collect()
}

/// The flags in an artifact's zip: every `.flaky.jsonl` entry, line by line.
fn flags(artifact: &Artifact, zip: &[u8]) -> Result<Vec<Flag>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))?;
    let mut found = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if !entry.name()?.ends_with(".flaky.jsonl") {
            continue;
        }
        let mut text = String::new();
        entry.take(ZIP_LIMIT).read_to_string(&mut text)?;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let line: Line = serde_json::from_str(line)?;
            found.push(Flag {
                test: line.test,
                project: line.project,
                error: line.error,
                artifact: artifact.id,
                run: artifact.workflow_run.as_ref().map(|run| run.id),
            });
        }
    }
    Ok(found)
}

/// Tests flagged by at least [`THRESHOLD`] different artifacts, in name order.
fn repeated(flags: &[Flag]) -> Vec<Repeat> {
    let mut by_test: BTreeMap<(&str, &str), Vec<&Flag>> = BTreeMap::new();
    for flag in flags {
        by_test
            .entry((&flag.test, &flag.project))
            .or_default()
            .push(flag);
    }
    by_test
        .into_iter()
        .filter_map(|((test, project), flags)| {
            let artifacts: BTreeSet<_> = flags.iter().map(|flag| flag.artifact).collect();
            (artifacts.len() >= THRESHOLD).then(|| Repeat {
                test: test.to_owned(),
                project: project.to_owned(),
                runs: flags
                    .iter()
                    .map(|flag| flag.run)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                errors: flags
                    .iter()
                    .map(|flag| flag.error.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            })
        })
        .collect()
}

fn plain(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// The issue title (stable, so a later night comments instead of opening another), its
/// body, and the note a later night adds.
fn text(repeat: &Repeat, repo: &str, flagged: usize) -> (String, String, String) {
    let title = format!(
        "Flaky e2e test: {} [{}]",
        plain(&repeat.test),
        repeat.project
    );
    let mut runs = String::new();
    for run in repeat.runs.iter().take(10) {
        runs.push_str(&format!(
            "- {}\n",
            run_link(repo, run.map(|id| id.to_string()).as_deref())
        ));
    }
    let mut errors = String::new();
    for error in repeat.errors.iter().take(5) {
        errors.push_str(&format!("- `{}`\n", plain(error).replace('`', "'")));
    }
    let facts = format!(
        "Flagged flaky {flagged} times in the last {WINDOW_DAYS} days: it failed, then passed on \
         its one immediate retry, so the runs below did not fail.\n\nRuns:\n{runs}\nFirst errors:\n{errors}"
    );
    let body = format!(
        "{facts}\nFind the cause from a failed run's trace, fix the test or the product, and \
         prove it with the `e2e-repeat` workflow (docs/flaky-tests.md): 30 passes in a row on \
         the project."
    );
    (title, body, format!("Flagged again.\n\n{facts}"))
}

fn gh_bytes(root: &Path, host: &str, endpoint: &str) -> Result<Vec<u8>> {
    let output = Command::new("gh")
        .current_dir(root)
        .args(["api", "--hostname", host, endpoint])
        .env_remove("GH_HOST")
        .env_remove("GH_REPO")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "gh api {endpoint}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(output.stdout)
}

/// The flaky-record artifacts of the window, newest first, from GitHub.
fn list(root: &Path, target: &Target, path: &str, today: i64) -> Result<Vec<Artifact>> {
    let mut found = Vec::new();
    for page in 1..=PAGE_CAP {
        let endpoint = format!("repos/{path}/actions/artifacts?per_page=100&page={page}");
        let listed: Page = serde_json::from_slice(&gh_bytes(root, &target.host, &endpoint)?)?;
        // The listing is newest first: a page that ends before the window ends the search.
        let ends_before = listed.artifacts.last().is_none_or(|artifact| {
            artifact
                .created_at
                .get(..10)
                .and_then(|day| date(day).ok())
                .is_some_and(|day| today - day > WINDOW_DAYS)
        });
        found.extend(listed.artifacts);
        if ends_before {
            break;
        }
    }
    Ok(recent(&found, today).into_iter().cloned().collect())
}

/// Opens or comments on one issue per test flagged in three runs within seven days.
pub(super) fn report(root: &Path) -> Result<()> {
    let target = Target::load(root)?;
    let path = target
        .repo
        .split_once('/')
        .map_or(target.repo.as_str(), |(_, path)| path)
        .to_owned();
    let mut found = Vec::new();
    for artifact in list(root, &target, &path, today()?)? {
        let zip = gh_bytes(
            root,
            &target.host,
            &format!("repos/{path}/actions/artifacts/{}/zip", artifact.id),
        )?;
        found.extend(flags(&artifact, &zip)?);
    }
    for repeat in repeated(&found) {
        let flagged = found
            .iter()
            .filter(|flag| flag.test == repeat.test && flag.project == repeat.project)
            .map(|flag| flag.artifact)
            .collect::<BTreeSet<_>>()
            .len();
        let (title, body, note) = text(&repeat, &target.repo, flagged);
        let labels = ["priority:medium".to_owned(), "area:e2e".to_owned()];
        let outcome = target.file(root, &title, &labels, &body, &note)?;
        println!("flaky: {title}: {outcome}");
    }
    println!(
        "flaky: {} test flag(s) in the last {WINDOW_DAYS} days, {} reported",
        found.len(),
        repeated(&found).len()
    );
    Ok(())
}

#[cfg(test)]
#[path = "tests/flaky.rs"]
mod tests;
